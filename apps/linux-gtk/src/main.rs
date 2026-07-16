//! Linux GTK4 shell.
//!
//! This dogfood client links the Rust rendering core directly rather than
//! crossing the `pdf-ffi` UniFFI boundary used by other platform shells.

#[cfg(target_os = "linux")]
fn main() -> gtk::glib::ExitCode {
    app::run()
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("linux-gtk is available only on Linux.");
}

#[cfg(target_os = "linux")]
mod app {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use gtk::prelude::*;
    use gtk::{
        gdk_pixbuf, gio, glib, Application, ApplicationWindow, Box as GtkBox, Button, Dialog,
        Entry, FileChooserAction, FileChooserNative, FileFilter, Label, Orientation, Picture,
        ResponseType, ScrolledWindow,
    };
    use pdf_render::{DocumentHandle, PdfiumRenderer, Priority, RenderError, RenderOptions};

    const APPLICATION_ID: &str = "org.vitela.Pdf";
    const POINTS_PER_INCH: f64 = 72.0;
    /// Fit-to-width DPI ceiling: a degenerate MediaBox (e.g. a page 1pt wide
    /// but thousands of points tall) would otherwise request an unbounded
    /// raster size from the render actor.
    const MAX_RENDER_DPI: f64 = 1440.0;

    pub fn run() -> glib::ExitCode {
        let application = Application::builder()
            .application_id(APPLICATION_ID)
            .build();
        application.connect_activate(build_ui);
        application.run()
    }

    /// The widgets and state a completed open updates, cloneable into signal
    /// handlers (GTK objects are internally reference-counted).
    #[derive(Clone)]
    struct Viewer {
        scroll: ScrolledWindow,
        picture: Picture,
        status: Label,
        open_document: Rc<RefCell<Option<DocumentHandle>>>,
    }

    /// A rendered page in `Send` form: produced on a worker thread, converted
    /// into a (non-`Send`) `Pixbuf` back on the GTK main thread.
    struct RenderedPage {
        document: DocumentHandle,
        width: u32,
        height: u32,
        stride: u32,
        pixels: Vec<u8>,
    }

    /// Snapshot of the viewport taken on the GTK thread before handing work
    /// to the render worker.
    #[derive(Clone, Copy)]
    struct FitRequest {
        /// Physical pixels available for the page: logical viewport width
        /// times the display scale factor, so HiDPI displays receive a
        /// full-density raster instead of an upscaled blurry one.
        available_width: u32,
        scale_factor: i32,
    }

    impl FitRequest {
        fn measure(viewer: &Viewer) -> Self {
            let scale_factor = viewer.scroll.scale_factor().max(1);
            FitRequest {
                available_width: (viewer.scroll.width().max(1) * scale_factor) as u32,
                scale_factor,
            }
        }
    }

    fn build_ui(application: &Application) {
        let window = ApplicationWindow::builder()
            .application(application)
            .default_width(1000)
            .default_height(800)
            .title("Vitela")
            .build();

        let open_button = Button::with_label("Open PDF");
        let status = Label::new(Some("Choose a PDF file to render page 1."));
        status.set_xalign(0.0);

        let picture = Picture::new();
        picture.set_can_shrink(true);
        picture.set_keep_aspect_ratio(true);

        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&picture)
            .build();

        let content = GtkBox::new(Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.append(&open_button);
        content.append(&status);
        content.append(&scroll);
        window.set_child(Some(&content));

        let viewer = Viewer {
            scroll,
            picture,
            status,
            open_document: Rc::new(RefCell::new(None)),
        };

        // `FileChooserNative` is not a widget: GTK holds no reference to it
        // while it is shown, so the shell must keep it alive here until the
        // response arrives or the dialog is destroyed before it can be used.
        let active_chooser: Rc<RefCell<Option<FileChooserNative>>> = Rc::new(RefCell::new(None));

        open_button.connect_clicked({
            let window = window.clone();
            let viewer = viewer.clone();
            let active_chooser = active_chooser.clone();
            move |_| {
                let filter = FileFilter::new();
                filter.set_name(Some("PDF files"));
                filter.add_mime_type("application/pdf");
                filter.add_pattern("*.pdf");
                filter.add_pattern("*.PDF");

                let chooser = FileChooserNative::new(
                    Some("Open PDF"),
                    Some(&window),
                    FileChooserAction::Open,
                    Some("Open"),
                    Some("Cancel"),
                );
                chooser.add_filter(&filter);
                chooser.connect_response({
                    let window = window.clone();
                    let viewer = viewer.clone();
                    let active_chooser = active_chooser.clone();
                    move |chooser, response| {
                        active_chooser.replace(None);
                        if response != ResponseType::Accept {
                            return;
                        }

                        let Some(path) = chooser.file().and_then(|file| file.path()) else {
                            viewer
                                .status
                                .set_text("The selected location is not a local file.");
                            return;
                        };
                        open_initial(&window, &viewer, path);
                    }
                });
                chooser.show();
                active_chooser.replace(Some(chooser));
            }
        });

        window.present();
    }

    /// Opens `path` without a password on a worker thread; falls through to
    /// the password prompt when pdfium rejects the (absent) password.
    fn open_initial(window: &ApplicationWindow, viewer: &Viewer, path: PathBuf) {
        let fit = FitRequest::measure(viewer);
        viewer.status.set_text("Rendering page 1...");
        glib::spawn_future_local({
            let window = window.clone();
            let viewer = viewer.clone();
            async move {
                match open_in_background(path.clone(), fit, None).await {
                    Ok(page) => show_page(&viewer, page, fit.scale_factor),
                    Err(RenderError::InvalidPassword) => {
                        prompt_for_password(&window, &viewer, path);
                    }
                    Err(error) => viewer
                        .status
                        .set_text(&format!("Could not open PDF: {error}")),
                }
            }
        });
    }

    /// Runs the open+render on a worker thread so the GTK main loop keeps
    /// painting and handling input while pdfium works.
    async fn open_in_background(
        path: PathBuf,
        fit: FitRequest,
        password: Option<String>,
    ) -> Result<RenderedPage, RenderError> {
        gio::spawn_blocking(move || open_page_one(&path, fit.available_width, password.as_deref()))
            .await
            .expect("page-open task panicked")
    }

    fn show_page(viewer: &Viewer, page: RenderedPage, scale_factor: i32) {
        if let Some(previous) = viewer.open_document.replace(Some(page.document)) {
            let _ = PdfiumRenderer::new().close_document(previous);
        }
        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(page.pixels),
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            page.width as i32,
            page.height as i32,
            page.stride as i32,
        );
        viewer.picture.set_pixbuf(Some(&pixbuf));
        // The raster is `scale_factor`x the logical target width: request the
        // logical size so it maps 1:1 onto physical pixels on HiDPI displays
        // and never exceeds the viewport width it was fitted to.
        viewer
            .picture
            .set_width_request(pixbuf.width() / scale_factor);
        viewer
            .picture
            .set_height_request(pixbuf.height() / scale_factor);
        viewer
            .status
            .set_text("Showing page 1 fitted to the available width.");
    }

    fn prompt_for_password(window: &ApplicationWindow, viewer: &Viewer, path: PathBuf) {
        let dialog = Dialog::builder()
            .transient_for(window)
            .modal(true)
            .title("Password required")
            .build();
        dialog.add_button("Cancel", ResponseType::Cancel);
        dialog.add_button("Open", ResponseType::Accept);
        dialog.set_default_response(ResponseType::Accept);

        let password_entry = Entry::builder()
            .visibility(false)
            .activates_default(true)
            .build();
        let error_label = Label::new(None);
        error_label.set_xalign(0.0);
        dialog.content_area().append(&password_entry);
        dialog.content_area().append(&error_label);
        password_entry.grab_focus();

        dialog.connect_response({
            let viewer = viewer.clone();
            move |dialog, response| {
                if response != ResponseType::Accept {
                    viewer.status.set_text("Password entry cancelled.");
                    dialog.close();
                    return;
                }

                let fit = FitRequest::measure(&viewer);
                viewer.status.set_text("Rendering page 1...");
                glib::spawn_future_local({
                    let dialog = dialog.clone();
                    let viewer = viewer.clone();
                    let password_entry = password_entry.clone();
                    let error_label = error_label.clone();
                    let path = path.clone();
                    async move {
                        let password = password_entry.text().to_string();
                        match open_in_background(path, fit, Some(password)).await {
                            Ok(page) => {
                                show_page(&viewer, page, fit.scale_factor);
                                dialog.close();
                            }
                            Err(RenderError::InvalidPassword) => {
                                viewer.status.set_text("Waiting for the document password.");
                                error_label.set_text("The password is incorrect. Try again.");
                                password_entry.set_text("");
                                password_entry.grab_focus();
                            }
                            Err(error) => {
                                viewer
                                    .status
                                    .set_text(&format!("Could not open PDF: {error}"));
                                dialog.close();
                            }
                        }
                    }
                });
            }
        });
        dialog.present();
    }

    /// Worker-thread entry: opens the document and renders page 1 fitted to
    /// `available_width` physical pixels, closing the document again if the
    /// render fails.
    fn open_page_one(
        path: &Path,
        available_width: u32,
        password: Option<&str>,
    ) -> Result<RenderedPage, RenderError> {
        let renderer = PdfiumRenderer::new();
        let document = renderer.open_document(path, password)?;

        match render_page_one(&renderer, document, available_width) {
            Ok(page) => Ok(page),
            Err(error) => {
                let _ = renderer.close_document(document);
                Err(error)
            }
        }
    }

    fn render_page_one(
        renderer: &PdfiumRenderer,
        document: DocumentHandle,
        available_width: u32,
    ) -> Result<RenderedPage, RenderError> {
        let (page_width_pt, _) = renderer.page_size(document, 0, Priority::Visible).wait()?;
        // floor(), not ceil(): overshooting the DPI would render wider than
        // the viewport the page is being fitted to.
        let dpi = ((f64::from(available_width) / f64::from(page_width_pt)) * POINTS_PER_INCH)
            .floor()
            .clamp(1.0, MAX_RENDER_DPI) as u32;

        let bitmap = renderer
            .render_page(
                document,
                0,
                dpi,
                None,
                RenderOptions::default(),
                Priority::Visible,
            )
            .wait()?;
        Ok(RenderedPage {
            document,
            width: bitmap.width()?,
            height: bitmap.height()?,
            stride: bitmap.stride()?,
            pixels: bitmap.get_pixels()?,
        })
    }
}
