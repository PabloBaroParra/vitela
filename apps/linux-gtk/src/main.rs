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
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use gtk::gdk::prelude::GdkCairoContextExt;
    use gtk::prelude::*;
    use gtk::{
        gdk_pixbuf, gio, glib, Application, ApplicationWindow, Box as GtkBox, Button, Dialog,
        Entry, FileChooserAction, FileChooserNative, FileFilter, Label, Orientation, Picture,
        PrintContext, PrintOperation, PrintOperationAction, PrintOperationResult, ResponseType,
        ScrolledWindow,
    };
    use pdf_render::{
        CancellationHandle, DocumentHandle, PdfiumRenderer, Priority, RenderError, RenderOptions,
        TextMatch,
    };

    const APPLICATION_ID: &str = "org.vitela.Pdf";
    const POINTS_PER_INCH: f64 = 72.0;
    const PAGE_GAP: i32 = 12;
    const PREFETCH_PAGES: usize = 1;
    const CACHE_PAGES: usize = 3;
    const MAX_RASTER_DIMENSION: u32 = 16_384;
    const MAX_RASTER_PIXELS: u64 = 32 * 1024 * 1024;
    const MAX_RASTER_BYTES: u64 = 128 * 1024 * 1024;
    /// Fit-to-width DPI ceiling: a degenerate MediaBox (e.g. a page 1pt wide
    /// but thousands of points tall) would otherwise request an unbounded
    /// raster size from the render actor.
    const MAX_RENDER_DPI: f64 = 1440.0;
    /// Fixed rasterization DPI for printing. Unlike the viewer, printing does
    /// not fit to a widget width: it renders each page once at a print-quality
    /// resolution and lets cairo scale the bitmap onto the paper.
    const PRINT_DPI: u32 = 300;

    pub fn run() -> glib::ExitCode {
        let application = Application::builder()
            .application_id(APPLICATION_ID)
            .build();
        application.connect_activate(build_ui);
        application.run()
    }

    #[derive(Clone)]
    struct Viewer {
        scroll: ScrolledWindow,
        pages: GtkBox,
        status: Label,
        search_entry: Entry,
        find_previous: Button,
        find_next: Button,
        print_button: Button,
        state: Rc<RefCell<ViewerState>>,
    }

    struct ViewerState {
        generation: u64,
        session: Option<DocumentSession>,
    }

    struct DocumentSession {
        document: DocumentHandle,
        physical_width: u32,
        scale_factor: i32,
        pages: Vec<PageSlot>,
        /// Cached logical heights, one per page — recomputed only when the fit
        /// changes (`show_document`/`refresh_layout`) so the per-scroll
        /// `update_viewport` never re-queries every widget's size request.
        page_heights: Vec<i32>,
        /// The last `(first, last)` range reported to the status label, so a
        /// scroll tick that doesn't move the visible range skips the redundant
        /// `format!` + `set_text`.
        last_visible: Option<(usize, usize)>,
        /// Matches for the last query run against *this* document. Lives in the
        /// session so replacing the document drops them with it.
        search: Option<SearchState>,
        /// Id of the most recently issued search. A slow search whose id is no
        /// longer current has been superseded by a later query and must not
        /// overwrite its results.
        next_search_id: u64,
        active: HashMap<usize, ActiveRender>,
        next_render_id: u64,
    }

    struct SearchState {
        query: String,
        matches: Vec<TextMatch>,
        current: usize,
    }

    struct ActiveRender {
        id: u64,
        cancellation: CancellationHandle,
    }

    struct PageSlot {
        picture: Picture,
        width_pt: f32,
        height_pt: f32,
        state: PageState,
    }

    /// Render lifecycle of a single page slot. `Skipped`/`Failed` are terminal
    /// for the current fit: they keep `update_viewport` from re-queuing a job
    /// that can only be rejected or fail again. A new fit (`refresh_layout`)
    /// resets every slot to `Idle`, giving oversized/failed pages one retry at
    /// the new size.
    #[derive(Clone, Copy, PartialEq)]
    enum PageState {
        Idle,
        Rendered,
        Skipped,
        Failed,
    }

    /// A rendered page in `Send` form: produced on a worker thread, converted
    /// into a non-`Send` pixbuf only on the GTK main thread.
    struct RenderedPage {
        width: u32,
        height: u32,
        stride: u32,
        pixels: Vec<u8>,
    }

    struct OpenedDocument {
        document: DocumentHandle,
        page_sizes: Vec<(f32, f32)>,
    }

    #[derive(Clone, Copy)]
    struct FitRequest {
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
        let status = Label::new(Some("Choose a PDF file to view."));
        status.set_xalign(0.0);

        // Exact, case-sensitive search: the same matcher `pdf-ffi` uses, so
        // this shell and the other platforms agree on what a match is.
        let search_entry = Entry::builder()
            .placeholder_text("Find in document")
            .hexpand(true)
            .build();
        let find_previous = Button::with_label("Previous");
        let find_next = Button::with_label("Next");
        find_previous.set_sensitive(false);
        find_next.set_sensitive(false);

        let print_button = Button::with_label("Print");
        print_button.set_sensitive(false);

        let toolbar = GtkBox::new(Orientation::Horizontal, 8);
        toolbar.append(&open_button);
        toolbar.append(&search_entry);
        toolbar.append(&find_previous);
        toolbar.append(&find_next);
        toolbar.append(&print_button);

        let pages = GtkBox::new(Orientation::Vertical, PAGE_GAP);
        pages.set_halign(gtk::Align::Center);
        let scroll = ScrolledWindow::builder()
            .hexpand(true)
            .vexpand(true)
            .child(&pages)
            .build();

        let content = GtkBox::new(Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        content.append(&toolbar);
        content.append(&status);
        content.append(&scroll);
        window.set_child(Some(&content));

        let viewer = Viewer {
            scroll,
            pages,
            status,
            search_entry,
            find_previous,
            find_next,
            print_button,
            state: Rc::new(RefCell::new(ViewerState {
                generation: 0,
                session: None,
            })),
        };
        connect_viewport_updates(&viewer);
        connect_search(&viewer);
        viewer.print_button.connect_clicked({
            let window = window.clone();
            let viewer = viewer.clone();
            move |_| print_document(&window, &viewer)
        });

        // `FileChooserNative` is not a widget: GTK holds no reference to it
        // while it is shown, so the shell must keep it alive here until the
        // response arrives or the dialog is destroyed before it can be used.
        let active_chooser: Rc<RefCell<Option<FileChooserNative>>> = Rc::new(RefCell::new(None));
        open_button.connect_clicked({
            let window = window.clone();
            let viewer = viewer.clone();
            let active_chooser = active_chooser.clone();
            move |_| show_file_chooser(&window, &viewer, &active_chooser)
        });

        window.present();
    }

    fn connect_search(viewer: &Viewer) {
        viewer.search_entry.connect_activate({
            let viewer = viewer.clone();
            move |_| run_search(&viewer)
        });
        viewer.find_next.connect_clicked({
            let viewer = viewer.clone();
            move |_| step_match(&viewer, 1)
        });
        viewer.find_previous.connect_clicked({
            let viewer = viewer.clone();
            move |_| step_match(&viewer, -1)
        });
    }

    fn connect_viewport_updates(viewer: &Viewer) {
        let adjustment = viewer.scroll.vadjustment();
        adjustment.connect_value_changed({
            let viewer = viewer.clone();
            move |_| update_viewport(&viewer)
        });
        adjustment.connect_page_size_notify({
            let viewer = viewer.clone();
            move |_| update_viewport(&viewer)
        });
        // GtkWidget has no "width" GObject property, so `notify::width` would
        // never fire. The horizontal adjustment's page size tracks the
        // viewport width and changes on every horizontal resize — the correct
        // signal to re-fit pages to the new available width.
        let hadjustment = viewer.scroll.hadjustment();
        hadjustment.connect_page_size_notify({
            let viewer = viewer.clone();
            move |_| refresh_layout(&viewer)
        });
    }

    fn show_file_chooser(
        window: &ApplicationWindow,
        viewer: &Viewer,
        active_chooser: &Rc<RefCell<Option<FileChooserNative>>>,
    ) {
        let filter = FileFilter::new();
        filter.set_name(Some("PDF files"));
        filter.add_mime_type("application/pdf");
        filter.add_pattern("*.pdf");
        filter.add_pattern("*.PDF");

        let chooser = FileChooserNative::new(
            Some("Open PDF"),
            Some(window),
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

    fn open_initial(window: &ApplicationWindow, viewer: &Viewer, path: PathBuf) {
        let generation = begin_loading(viewer);
        viewer.status.set_text("Opening PDF...");
        glib::spawn_future_local({
            let window = window.clone();
            let viewer = viewer.clone();
            async move {
                match open_in_background(path.clone(), None).await {
                    Ok(document) if is_current(&viewer, generation) => {
                        show_document(&viewer, generation, document);
                    }
                    Ok(document) => close_document_in_background(document.document),
                    Err(RenderError::InvalidPassword) if is_current(&viewer, generation) => {
                        prompt_for_password(&window, &viewer, path, generation);
                    }
                    Err(error) if is_current(&viewer, generation) => viewer
                        .status
                        .set_text(&format!("Could not open PDF: {error}")),
                    Err(_) => {}
                }
            }
        });
    }

    /// Marks the start of a new open attempt and returns its generation.
    ///
    /// This does NOT touch the currently displayed document: a failed or
    /// superseded open must leave the previous document on screen. The old
    /// session is replaced only once the new one opens successfully, in
    /// [`show_document`]. The bumped generation lets [`is_current`] discard the
    /// results of any open this one supersedes.
    fn begin_loading(viewer: &Viewer) -> u64 {
        let mut state = viewer.state.borrow_mut();
        state.generation += 1;
        state.generation
    }

    fn is_current(viewer: &Viewer, generation: u64) -> bool {
        viewer.state.borrow().generation == generation
    }

    async fn open_in_background(
        path: PathBuf,
        password: Option<String>,
    ) -> Result<OpenedDocument, RenderError> {
        gio::spawn_blocking(move || open_document(&path, password.as_deref()))
            .await
            .expect("document-open task panicked")
    }

    fn open_document(path: &Path, password: Option<&str>) -> Result<OpenedDocument, RenderError> {
        let renderer = PdfiumRenderer::new();
        let document = renderer.open_document(path, password)?;
        // One batched actor round-trip for every page size, instead of N
        // serialized `page_size` round-trips — first paint no longer waits on
        // a per-page metadata sweep for large documents.
        match renderer.page_sizes(document, Priority::Visible).wait() {
            Ok(page_sizes) => Ok(OpenedDocument {
                document,
                page_sizes,
            }),
            Err(error) => {
                let _ = renderer.close_document(document);
                Err(error)
            }
        }
    }

    fn show_document(viewer: &Viewer, generation: u64, document: OpenedDocument) {
        if !is_current(viewer, generation) {
            close_document_in_background(document.document);
            return;
        }

        // The new document opened successfully: only now replace the previous
        // one. Cancel its in-flight renders and close it, then clear its page
        // widgets before building the new layout.
        {
            let mut state = viewer.state.borrow_mut();
            if let Some(session) = state.session.take() {
                for active in session.active.values() {
                    active.cancellation.cancel();
                }
                close_document_in_background(session.document);
            }
        }
        while let Some(child) = viewer.pages.first_child() {
            viewer.pages.remove(&child);
        }

        let fit = FitRequest::measure(viewer);
        let mut slots = Vec::with_capacity(document.page_sizes.len());
        let mut page_heights = Vec::with_capacity(document.page_sizes.len());
        for (width_pt, height_pt) in document.page_sizes {
            let picture = Picture::new();
            picture.set_can_shrink(true);
            picture.set_keep_aspect_ratio(true);
            let logical_height = set_placeholder_size(&picture, width_pt, height_pt, fit);
            viewer.pages.append(&picture);
            slots.push(PageSlot {
                picture,
                width_pt,
                height_pt,
                state: PageState::Idle,
            });
            page_heights.push(logical_height);
        }

        let page_count = slots.len();
        viewer.state.borrow_mut().session = Some(DocumentSession {
            document: document.document,
            physical_width: fit.available_width,
            scale_factor: fit.scale_factor,
            pages: slots,
            page_heights,
            last_visible: None,
            search: None,
            next_search_id: 0,
            active: HashMap::new(),
            next_render_id: 0,
        });
        // A new document invalidates the previous document's matches.
        update_search_controls(viewer);
        viewer.print_button.set_sensitive(page_count > 0);
        if page_count == 0 {
            viewer.status.set_text("The PDF contains no pages.");
        } else {
            update_viewport(viewer);
        }
    }

    /// Sizes a page's placeholder to the logical fit width and returns the
    /// logical height it set, so callers can cache it in `page_heights`.
    fn set_placeholder_size(
        picture: &Picture,
        width_pt: f32,
        height_pt: f32,
        fit: FitRequest,
    ) -> i32 {
        let logical_width = (fit.available_width / fit.scale_factor as u32).max(1) as i32;
        let logical_height = page_height(width_pt, height_pt, logical_width);
        picture.set_width_request(logical_width);
        picture.set_height_request(logical_height);
        logical_height
    }

    fn page_height(width_pt: f32, height_pt: f32, logical_width: i32) -> i32 {
        if width_pt <= 0.0 || height_pt <= 0.0 {
            return 1;
        }
        ((f64::from(logical_width) * f64::from(height_pt) / f64::from(width_pt)).round() as i32)
            .max(1)
    }

    fn refresh_layout(viewer: &Viewer) {
        let fit = FitRequest::measure(viewer);
        {
            let mut state = viewer.state.borrow_mut();
            let Some(session) = state.session.as_mut() else {
                return;
            };
            if session.physical_width == fit.available_width
                && session.scale_factor == fit.scale_factor
            {
                return;
            }
            session.physical_width = fit.available_width;
            session.scale_factor = fit.scale_factor;
            for active in session.active.values() {
                active.cancellation.cancel();
            }
            session.active.clear();
            session.last_visible = None;
            for index in 0..session.pages.len() {
                let page = &mut session.pages[index];
                page.picture.set_pixbuf(None);
                page.state = PageState::Idle;
                let logical_height =
                    set_placeholder_size(&page.picture, page.width_pt, page.height_pt, fit);
                session.page_heights[index] = logical_height;
            }
        }
        update_viewport(viewer);
    }

    fn update_viewport(viewer: &Viewer) {
        let adjustment = viewer.scroll.vadjustment();
        let mut jobs = Vec::new();
        let mut rejected_visible_page = None;
        let visible = {
            let mut state = viewer.state.borrow_mut();
            let Some(session) = state.session.as_mut() else {
                return;
            };
            let Some((first, last)) = visible_range(
                &session.page_heights,
                adjustment.value(),
                adjustment.page_size(),
            ) else {
                return;
            };
            let page_count = session.pages.len();
            let render_range = nearby_range(first, last, page_count, PREFETCH_PAGES);
            let cache_range = nearby_range(first, last, page_count, CACHE_PAGES);

            // Cancel and drop renders that scrolled out of the prefetch window;
            // `Range::contains` is an allocation-free membership test.
            session.active.retain(|page_index, active| {
                if render_range.contains(page_index) {
                    true
                } else {
                    active.cancellation.cancel();
                    false
                }
            });
            for (page_index, page) in session.pages.iter_mut().enumerate() {
                if !cache_range.contains(&page_index) && page.state == PageState::Rendered {
                    page.picture.set_pixbuf(None);
                    page.state = PageState::Idle;
                }
            }
            let document = session.document;
            let physical_width = session.physical_width;
            for page_index in render_range {
                if session.pages[page_index].state != PageState::Idle
                    || session.active.contains_key(&page_index)
                {
                    continue;
                }
                let (width_pt, height_pt) = {
                    let page = &session.pages[page_index];
                    (page.width_pt, page.height_pt)
                };
                let dpi = dpi_for_width(width_pt, physical_width);
                if raster_dimensions(width_pt, height_pt, dpi).is_some() {
                    let priority = if (first..=last).contains(&page_index) {
                        Priority::Visible
                    } else {
                        Priority::Prefetch
                    };
                    jobs.push((document, page_index, dpi, priority));
                } else {
                    // Terminal for this fit: don't re-detect it every tick, and
                    // only report it if it is actually on screen (not merely a
                    // prefetch neighbour hijacking the status line).
                    session.pages[page_index].state = PageState::Skipped;
                    if (first..=last).contains(&page_index) {
                        rejected_visible_page = Some(page_index);
                    }
                }
            }

            let range_changed = session.last_visible != Some((first, last));
            session.last_visible = Some((first, last));
            range_changed.then_some((first, last, page_count))
        };

        if let Some((first, last, page_count)) = visible {
            viewer.status.set_text(&format!(
                "Showing pages {}-{} of {page_count}.",
                first + 1,
                last + 1
            ));
        }
        if let Some(page_index) = rejected_visible_page {
            viewer.status.set_text(&format!(
                "Page {} cannot be rendered safely at this size.",
                page_index + 1
            ));
        }
        for (document, page_index, dpi, priority) in jobs {
            schedule_render(viewer, document, page_index, dpi, priority);
        }
    }

    fn visible_range(
        page_heights: &[i32],
        viewport_top: f64,
        viewport_height: f64,
    ) -> Option<(usize, usize)> {
        let viewport_bottom = viewport_top + viewport_height.max(1.0);
        let mut page_top = 0.0;
        let mut first = None;
        let mut last = 0;
        for (index, height) in page_heights.iter().enumerate() {
            let page_bottom = page_top + f64::from(*height);
            if page_bottom >= viewport_top && page_top <= viewport_bottom {
                first.get_or_insert(index);
                last = index;
            }
            page_top = page_bottom + f64::from(PAGE_GAP);
        }
        first.map(|first| (first, last))
    }

    fn nearby_range(
        first: usize,
        last: usize,
        page_count: usize,
        radius: usize,
    ) -> std::ops::Range<usize> {
        first.saturating_sub(radius)..(last + radius + 1).min(page_count)
    }

    fn dpi_for_width(width_pt: f32, available_width: u32) -> u32 {
        ((f64::from(available_width) / f64::from(width_pt)) * POINTS_PER_INCH)
            .floor()
            .clamp(1.0, MAX_RENDER_DPI) as u32
    }

    fn raster_dimensions(width_pt: f32, height_pt: f32, dpi: u32) -> Option<(u32, u32)> {
        let scale = f64::from(dpi) / POINTS_PER_INCH;
        let width = (f64::from(width_pt) * scale).round();
        let height = (f64::from(height_pt) * scale).round();
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }
        let (width, height) = (width as u32, height as u32);
        let pixels = u64::from(width).checked_mul(u64::from(height))?;
        let bytes = pixels.checked_mul(4)?;
        (width <= MAX_RASTER_DIMENSION
            && height <= MAX_RASTER_DIMENSION
            && pixels <= MAX_RASTER_PIXELS
            && bytes <= MAX_RASTER_BYTES)
            .then_some((width, height))
    }

    fn schedule_render(
        viewer: &Viewer,
        document: DocumentHandle,
        page_index: usize,
        dpi: u32,
        priority: Priority,
    ) {
        let handle = PdfiumRenderer::new().render_page(
            document,
            page_index as u32,
            dpi,
            None,
            RenderOptions::default(),
            priority,
        );
        let cancellation = handle.cancellation_handle();
        // The session is identified by its document handle (pdfium hands out
        // monotonically increasing ids, so a replaced session never collides
        // with the one this render targets). That check alone tells a stale
        // render from a live one — no separate generation is threaded through.
        let render_id = {
            let mut state = viewer.state.borrow_mut();
            let Some(session) = state.session.as_mut() else {
                cancellation.cancel();
                return;
            };
            if session.document != document {
                cancellation.cancel();
                return;
            }
            let render_id = session.next_render_id;
            session.next_render_id += 1;
            session.active.insert(
                page_index,
                ActiveRender {
                    id: render_id,
                    cancellation,
                },
            );
            render_id
        };
        glib::spawn_future_local({
            let viewer = viewer.clone();
            async move {
                let result = gio::spawn_blocking(move || render_result(handle))
                    .await
                    .expect("page-render task panicked");
                apply_render_result(&viewer, document, page_index, render_id, result);
            }
        });
    }

    fn render_result(handle: pdf_render::RenderHandle) -> Result<RenderedPage, RenderError> {
        let bitmap = handle.wait()?;
        Ok(RenderedPage {
            width: bitmap.width()?,
            height: bitmap.height()?,
            stride: bitmap.stride()?,
            pixels: bitmap.get_pixels()?,
        })
    }

    fn apply_render_result(
        viewer: &Viewer,
        document: DocumentHandle,
        page_index: usize,
        render_id: u64,
        result: Result<RenderedPage, RenderError>,
    ) {
        let mut state = viewer.state.borrow_mut();
        let Some(session) = state.session.as_mut() else {
            return;
        };
        if session.document != document {
            return;
        }
        // Only the render currently recorded for this page may apply its
        // result; a superseded one (its slot re-scheduled after a refit) is
        // dropped. This is the same window `update_viewport` kept in the
        // `active` map, so the finished render is always still wanted — no
        // recompute of the visible range is needed to re-check that.
        if session.active.get(&page_index).map(|active| active.id) != Some(render_id) {
            return;
        }
        session.active.remove(&page_index);
        let page_count = session.pages.len();
        let Some(slot) = session.pages.get_mut(page_index) else {
            return;
        };
        match result {
            Ok(page) => {
                let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
                    &glib::Bytes::from_owned(page.pixels),
                    gdk_pixbuf::Colorspace::Rgb,
                    true,
                    8,
                    page.width as i32,
                    page.height as i32,
                    page.stride as i32,
                );
                slot.picture.set_pixbuf(Some(&pixbuf));
                slot.state = PageState::Rendered;
            }
            Err(RenderError::Cancelled) => {}
            Err(error) => {
                // Mark the slot failed so `update_viewport` doesn't re-queue the
                // same doomed render on every scroll tick. A refit resets it.
                slot.state = PageState::Failed;
                viewer.status.set_text(&format!(
                    "Could not render page {} of {page_count}: {error}",
                    page_index + 1,
                ));
            }
        }
    }

    /// Prints the open document via the platform print dialog.
    ///
    /// Printing is deliberately independent of the viewer's coalesced render
    /// queue: each page is rasterized once at [`PRINT_DPI`] on demand inside
    /// `draw_page`, so a page scrolling out of view can never supersede a page
    /// the printer still needs. `PrintOperation::run` drives a modal dialog on
    /// a nested main loop and returns only once printing finishes or is
    /// cancelled; the per-page render blocks that loop, which is acceptable for
    /// a modal operation the user has explicitly started.
    fn print_document(window: &ApplicationWindow, viewer: &Viewer) {
        let Some((document, page_sizes)) = ({
            let state = viewer.state.borrow();
            state.session.as_ref().map(|session| {
                let sizes: Vec<(f32, f32)> = session
                    .pages
                    .iter()
                    .map(|page| (page.width_pt, page.height_pt))
                    .collect();
                (session.document, sizes)
            })
        }) else {
            viewer.status.set_text("Open a PDF before printing.");
            return;
        };
        if page_sizes.is_empty() {
            viewer.status.set_text("The PDF has no pages to print.");
            return;
        }

        let operation = PrintOperation::new();
        operation.set_n_pages(page_sizes.len() as i32);
        operation.set_embed_page_setup(true);
        operation.connect_draw_page(move |_operation, context, page_number| {
            draw_print_page(document, &page_sizes, page_number, context);
        });

        match operation.run(PrintOperationAction::PrintDialog, Some(window)) {
            Ok(PrintOperationResult::Error) => viewer.status.set_text("Printing failed."),
            Ok(_) => {}
            Err(error) => viewer.status.set_text(&format!("Could not print: {error}")),
        }
    }

    /// Rasterizes one page at print quality and paints it onto the printer's
    /// cairo surface, scaled to fit the paper while preserving aspect ratio. A
    /// page that cannot be rendered safely (oversized raster) or fails to
    /// render is left blank rather than aborting the whole job.
    fn draw_print_page(
        document: DocumentHandle,
        page_sizes: &[(f32, f32)],
        page_number: i32,
        context: &PrintContext,
    ) {
        let Some(&(width_pt, height_pt)) = page_sizes.get(page_number as usize) else {
            return;
        };
        if raster_dimensions(width_pt, height_pt, PRINT_DPI).is_none() {
            return;
        }

        let handle = PdfiumRenderer::new().render_page(
            document,
            page_number as u32,
            PRINT_DPI,
            None,
            RenderOptions::default(),
            Priority::Visible,
        );
        let Ok(page) = render_result(handle) else {
            return;
        };

        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &glib::Bytes::from_owned(page.pixels),
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            page.width as i32,
            page.height as i32,
            page.stride as i32,
        );

        // The print context reports the imageable area in device units that
        // already match the rendered bitmap's pixel space, so fit the bitmap
        // to it by the smaller axis ratio and centre it on the longer one.
        let scale = (context.width() / f64::from(page.width))
            .min(context.height() / f64::from(page.height));
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }

        let cairo = context.cairo_context();
        if cairo.save().is_err() {
            return;
        }
        cairo.scale(scale, scale);
        cairo.set_source_pixbuf(&pixbuf, 0.0, 0.0);
        let _ = cairo.paint();
        let _ = cairo.restore();
    }

    fn run_search(viewer: &Viewer) {
        let query = viewer.search_entry.text().to_string();
        if query.is_empty() {
            clear_search(viewer);
            viewer.status.set_text("Enter text to find.");
            return;
        }
        let Some((document, search_id)) = begin_search(viewer) else {
            viewer.status.set_text("Open a PDF before searching.");
            return;
        };

        viewer
            .status
            .set_text(&format!("Searching for \"{query}\"..."));
        glib::spawn_future_local({
            let viewer = viewer.clone();
            let job_query = query.clone();
            async move {
                let result = gio::spawn_blocking(move || {
                    PdfiumRenderer::new()
                        .search(document, job_query, Priority::Visible)
                        .wait()
                })
                .await
                .expect("search task panicked");
                apply_search_result(&viewer, document, search_id, query, result);
            }
        });
    }

    /// Claims the next search id for the open document, marking any in-flight
    /// search as superseded.
    fn begin_search(viewer: &Viewer) -> Option<(DocumentHandle, u64)> {
        let mut state = viewer.state.borrow_mut();
        let session = state.session.as_mut()?;
        session.next_search_id += 1;
        Some((session.document, session.next_search_id))
    }

    fn apply_search_result(
        viewer: &Viewer,
        document: DocumentHandle,
        search_id: u64,
        query: String,
        result: Result<Vec<TextMatch>, RenderError>,
    ) {
        let mut found = false;
        let status = {
            let mut state = viewer.state.borrow_mut();
            let Some(session) = state.session.as_mut() else {
                return;
            };
            // The document was replaced while the search ran: its matches
            // address pages that are no longer on screen.
            if session.document != document {
                return;
            }
            // A later query was issued while this one ran: a slow search must
            // not clobber the results the user is actually looking at.
            if session.next_search_id != search_id {
                return;
            }
            match result {
                Ok(matches) if matches.is_empty() => {
                    session.search = None;
                    format!("No matches for \"{query}\".")
                }
                Ok(matches) => {
                    let status = search_status(&query, 0, matches.len());
                    session.search = Some(SearchState {
                        query,
                        matches,
                        current: 0,
                    });
                    found = true;
                    status
                }
                Err(error) => {
                    session.search = None;
                    format!("Could not search: {error}")
                }
            }
        };

        update_search_controls(viewer);
        // Scroll first: moving the adjustment fires `update_viewport`, which
        // writes its own "Showing pages" text. Setting the search status after
        // it lets the more specific message win.
        if found {
            scroll_to_current_match(viewer);
        }
        viewer.status.set_text(&status);
    }

    fn step_match(viewer: &Viewer, delta: i32) {
        let status = {
            let mut state = viewer.state.borrow_mut();
            let Some(session) = state.session.as_mut() else {
                return;
            };
            let Some(search) = session.search.as_mut() else {
                return;
            };
            if search.matches.is_empty() {
                return;
            }
            search.current = step_index(search.current, delta, search.matches.len());
            search_status(&search.query, search.current, search.matches.len())
        };

        scroll_to_current_match(viewer);
        viewer.status.set_text(&status);
    }

    fn scroll_to_current_match(viewer: &Viewer) {
        let target = {
            let state = viewer.state.borrow();
            let Some(session) = state.session.as_ref() else {
                return;
            };
            let Some(search) = session.search.as_ref() else {
                return;
            };
            let Some(found) = search.matches.get(search.current) else {
                return;
            };
            page_top(&session.page_heights, found.page_index as usize)
        };
        // The borrow above must end before this: `set_value` synchronously
        // emits `value_changed`, whose handler borrows the state again.
        viewer.scroll.vadjustment().set_value(target);
    }

    fn clear_search(viewer: &Viewer) {
        // Scoped explicitly: `update_search_controls` borrows the state again,
        // so this one must be released first.
        {
            let mut state = viewer.state.borrow_mut();
            if let Some(session) = state.session.as_mut() {
                session.search = None;
            }
        }
        update_search_controls(viewer);
    }

    fn update_search_controls(viewer: &Viewer) {
        let has_matches = viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.search.as_ref())
            .is_some_and(|search| !search.matches.is_empty());
        viewer.find_previous.set_sensitive(has_matches);
        viewer.find_next.set_sensitive(has_matches);
    }

    /// Distance from the top of the page box to the top of `page_index`,
    /// mirroring the stacking `visible_range` walks: each page contributes its
    /// height plus the box's inter-page gap.
    fn page_top(page_heights: &[i32], page_index: usize) -> f64 {
        page_heights
            .iter()
            .take(page_index)
            .map(|height| f64::from(*height) + f64::from(PAGE_GAP))
            .sum()
    }

    /// Steps a match index with wraparound, so Next on the last match returns
    /// to the first.
    fn step_index(current: usize, delta: i32, len: usize) -> usize {
        if len == 0 {
            return 0;
        }
        (current as i64 + i64::from(delta)).rem_euclid(len as i64) as usize
    }

    fn search_status(query: &str, index: usize, total: usize) -> String {
        format!("Match {} of {total} for \"{query}\".", index + 1)
    }

    fn close_document_in_background(document: DocumentHandle) {
        glib::spawn_future_local(async move {
            let _ =
                gio::spawn_blocking(move || PdfiumRenderer::new().close_document(document)).await;
        });
    }

    fn prompt_for_password(
        window: &ApplicationWindow,
        viewer: &Viewer,
        path: PathBuf,
        generation: u64,
    ) {
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

                viewer.status.set_text("Opening password-protected PDF...");
                dialog.set_sensitive(false);
                glib::spawn_future_local({
                    let dialog = dialog.clone();
                    let viewer = viewer.clone();
                    let password_entry = password_entry.clone();
                    let error_label = error_label.clone();
                    let path = path.clone();
                    async move {
                        let password = password_entry.text().to_string();
                        match open_in_background(path, Some(password)).await {
                            Ok(document) if is_current(&viewer, generation) => {
                                show_document(&viewer, generation, document);
                                dialog.close();
                            }
                            Ok(document) => close_document_in_background(document.document),
                            Err(RenderError::InvalidPassword)
                                if is_current(&viewer, generation) =>
                            {
                                dialog.set_sensitive(true);
                                viewer.status.set_text("Waiting for the document password.");
                                error_label.set_text("The password is incorrect. Try again.");
                                password_entry.set_text("");
                                password_entry.grab_focus();
                            }
                            Err(error) if is_current(&viewer, generation) => {
                                viewer
                                    .status
                                    .set_text(&format!("Could not open PDF: {error}"));
                                dialog.close();
                            }
                            Err(_) => {}
                        }
                    }
                });
            }
        });
        dialog.present();
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn visible_range_includes_pages_intersecting_the_viewport_edges() {
            // Viewport [95, 115): page 0 spans [0, 100), page 1 [112, 212)
            // after the 12pt gap — both touch the viewport, page 2 does not.
            assert_eq!(visible_range(&[100, 100, 100], 95.0, 20.0), Some((0, 1)));
        }

        #[test]
        fn visible_range_is_empty_without_pages() {
            assert_eq!(visible_range(&[], 0.0, 20.0), None);
        }

        #[test]
        fn nearby_range_stays_within_document_bounds() {
            assert_eq!(nearby_range(0, 0, 3, 3), 0..3);
        }

        #[test]
        fn raster_dimensions_rejects_an_oversized_page() {
            assert_eq!(raster_dimensions(72.0, 72_000.0, 72), None);
        }

        #[test]
        fn page_top_stacks_heights_and_gaps_like_visible_range() {
            // Page 0 starts at 0; page 2 sits below two pages and two gaps.
            assert_eq!(page_top(&[100, 100, 100], 0), 0.0);
            assert_eq!(
                page_top(&[100, 100, 100], 2),
                2.0 * (100.0 + f64::from(PAGE_GAP))
            );
        }

        #[test]
        fn step_index_wraps_around_both_ends() {
            assert_eq!(step_index(2, 1, 3), 0);
            assert_eq!(step_index(0, -1, 3), 2);
            assert_eq!(step_index(0, 1, 3), 1);
        }

        #[test]
        fn step_index_is_safe_without_matches() {
            assert_eq!(step_index(0, 1, 0), 0);
        }

        #[test]
        fn search_status_is_one_based_for_humans() {
            assert_eq!(search_status("hi", 0, 3), "Match 1 of 3 for \"hi\".");
        }
    }
}
