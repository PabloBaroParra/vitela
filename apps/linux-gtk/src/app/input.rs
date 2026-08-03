//! Clipboard and file-drop adapters for image stamps and document opening.

use std::path::PathBuf;

use gtk::prelude::*;
use gtk::{gdk, gio, glib, DrawingArea, DropTarget};
use image::ImageFormat;

use super::annotations;
use super::document::open_file;
use super::selection;
use super::state::Viewer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileDropKind {
    Pdf,
    Image,
    Unknown,
}

/// Wires Ctrl+V as a window action. It requests a texture only; notably, it
/// never reads clipboard text, so a URL cannot become an implicit download.
pub(crate) fn connect_paste(
    application: &gtk::Application,
    window: &gtk::ApplicationWindow,
    viewer: &Viewer,
) {
    let paste = gio::SimpleAction::new("paste", None);
    paste.connect_activate({
        let viewer = viewer.clone();
        move |_, _| paste_image(&viewer)
    });
    window.add_action(&paste);
    application.set_accels_for_action("win.paste", &["<Control>v"]);
}

/// Makes every page's input layer accept local file drops. Coordinates are
/// passed straight through the existing widget-to-PDF transform.
pub(crate) fn connect_file_drop(area: &DrawingArea, viewer: &Viewer, page_index: usize) {
    let target = DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    target.connect_drop({
        let viewer = viewer.clone();
        move |_, value, x, y| {
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            let Some(file) = files.files().into_iter().next() else {
                return false;
            };
            drop_file(&viewer, page_index, x, y, file);
            true
        }
    });
    area.add_controller(target);
}

/// The empty document area has no page input layer, so it needs its own target
/// for opening a PDF before the first document exists.
pub(crate) fn connect_window_file_drop(area: &gtk::Overlay, viewer: &Viewer) {
    let target = DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    target.connect_drop({
        let viewer = viewer.clone();
        move |_, value, _, _| {
            let Ok(files) = value.get::<gdk::FileList>() else {
                return false;
            };
            let Some(file) = files.files().into_iter().next() else {
                return false;
            };
            let Some(path) = file.path() else {
                viewer
                    .status
                    .set_text("Only local PDF and image files can be dropped.");
                return true;
            };
            load_window_drop(&viewer, path);
            true
        }
    });
    area.add_controller(target);
}

fn paste_image(viewer: &Viewer) {
    let Some((session_id, page_index, point)) = paste_target(viewer) else {
        viewer
            .status
            .set_text("Open a PDF before pasting an image.");
        return;
    };
    let clipboard = viewer.scroll.clipboard();
    clipboard.read_texture_async(None::<&gio::Cancellable>, {
        let viewer = viewer.clone();
        move |result| match result.ok().flatten() {
            Some(texture) if is_current_session(&viewer, session_id) => {
                annotations::stamp_from_image_bytes(
                    &viewer,
                    page_index,
                    point,
                    texture.save_to_png_bytes().as_ref().to_vec(),
                )
            }
            None => viewer
                .status
                .set_text("Clipboard does not contain a bitmap image."),
            Some(_) => {}
        }
    });
}

fn paste_target(viewer: &Viewer) -> Option<(u64, usize, (f64, f64))> {
    let state = viewer.state.borrow();
    let session = state.session.as_ref()?;
    let page_index = session.last_visible.map_or(0, |(first, _)| first);
    let page = session.pages.get(page_index)?;
    Some((
        state.session_id,
        page_index,
        (
            f64::from(page.width_pt) / 2.0,
            f64::from(page.height_pt) / 2.0,
        ),
    ))
}

fn drop_file(viewer: &Viewer, page_index: usize, x: f64, y: f64, file: gio::File) {
    let Some(path) = file.path() else {
        viewer
            .status
            .set_text("Only local PDF and image files can be dropped.");
        return;
    };
    let Some(session_id) = current_session_id(viewer) else {
        return;
    };
    let Some(point) = selection::pointer_to_pdf(viewer, page_index, x, y) else {
        viewer
            .status
            .set_text("The page is not ready for image placement.");
        return;
    };
    viewer.status.set_text("Loading dropped file...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            match read_dropped_file(path).await {
                Ok((path, bytes)) => match classify_file_contents(&bytes) {
                    FileDropKind::Pdf if is_current_session(&viewer, session_id) => {
                        open_file(&viewer, path)
                    }
                    FileDropKind::Pdf => {}
                    FileDropKind::Image if is_current_session(&viewer, session_id) => {
                        annotations::stamp_from_image_bytes(&viewer, page_index, point, bytes)
                    }
                    FileDropKind::Image => {}
                    FileDropKind::Unknown => viewer
                        .status
                        .set_text("Dropped file is neither a PDF nor a supported image."),
                },
                Err(error) => viewer
                    .status
                    .set_text(&format!("Could not read dropped file: {error}")),
            }
        }
    });
}

fn load_window_drop(viewer: &Viewer, path: PathBuf) {
    viewer.status.set_text("Loading dropped file...");
    glib::spawn_future_local({
        let viewer = viewer.clone();
        async move {
            match read_dropped_file(path).await {
                Ok((path, bytes)) => match classify_file_contents(&bytes) {
                    FileDropKind::Pdf => open_file(&viewer, path),
                    FileDropKind::Image => viewer
                        .status
                        .set_text("Drop an image directly onto an open PDF page."),
                    FileDropKind::Unknown => viewer
                        .status
                        .set_text("Dropped file is neither a PDF nor a supported image."),
                },
                Err(error) => viewer
                    .status
                    .set_text(&format!("Could not read dropped file: {error}")),
            }
        }
    });
}

async fn read_dropped_file(path: PathBuf) -> Result<(PathBuf, Vec<u8>), String> {
    let read_path = path.clone();
    gio::spawn_blocking(move || std::fs::read(&read_path))
        .await
        .map_err(|_| "file loading stopped unexpectedly".to_string())?
        .map(|bytes| (path, bytes))
        .map_err(|error| error.to_string())
}

fn current_session_id(viewer: &Viewer) -> Option<u64> {
    let state = viewer.state.borrow();
    state.session.as_ref().map(|_| state.session_id)
}

fn is_current_session(viewer: &Viewer, session_id: u64) -> bool {
    let state = viewer.state.borrow();
    session_matches(session_id, state.session.as_ref().map(|_| state.session_id))
}

fn session_matches(captured: u64, current: Option<u64>) -> bool {
    current == Some(captured)
}

/// Routes dropped bytes by signature alone.
///
/// Deliberately *not* a full decode: this runs back on the main context after
/// the read completes, so decoding a large photo here would stall the UI — and
/// the bytes get decoded again by the core stamp builder anyway. Validity is
/// that builder's answer to give; it decodes once and returns a typed error the
/// shell already reports.
///
/// The accepted formats mirror the ones `pdf-annotate` enables, so anything
/// classified here as an image can actually become a stamp.
fn classify_file_contents(bytes: &[u8]) -> FileDropKind {
    if bytes.starts_with(b"%PDF-") {
        return FileDropKind::Pdf;
    }
    match image::guess_format(bytes) {
        Ok(ImageFormat::Png | ImageFormat::Jpeg) => FileDropKind::Image,
        _ => FileDropKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::{classify_file_contents, session_matches, FileDropKind};

    #[test]
    fn pdf_content_routes_to_document_opening_regardless_of_filename() {
        assert_eq!(
            classify_file_contents(b"%PDF-1.7\nexample"),
            FileDropKind::Pdf
        );
    }

    #[test]
    fn known_image_content_routes_to_stamp_placement() {
        const PNG: &[u8] = &[
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 4, 0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 156, 99, 252, 255, 31,
            0, 3, 3, 1, 255, 165, 70, 232, 7, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
        ];

        assert_eq!(classify_file_contents(PNG), FileDropKind::Image);
    }

    #[test]
    fn content_without_a_known_signature_is_rejected_before_decoding() {
        assert_eq!(
            classify_file_contents(b"just some text, not a document"),
            FileDropKind::Unknown
        );
    }

    /// A signature the core stamp builder cannot decode is refused here, so the
    /// user gets "neither a PDF nor a supported image" instead of a decoder
    /// error from a format the shell was never going to support.
    #[test]
    fn an_image_format_the_stamp_builder_lacks_is_refused_up_front() {
        assert_eq!(classify_file_contents(b"GIF89a...."), FileDropKind::Unknown);
    }

    /// Classification stops at the signature: a corrupt PNG still routes to the
    /// stamp path, where the core builder decodes it once and reports why.
    #[test]
    fn a_truncated_image_defers_validation_to_the_stamp_builder() {
        assert_eq!(
            classify_file_contents(b"\x89PNG\r\n\x1a\nnot a complete image"),
            FileDropKind::Image
        );
    }

    #[test]
    fn a_replaced_session_rejects_a_stale_callback() {
        assert!(!session_matches(4, Some(5)));
    }

    #[test]
    fn a_closed_document_rejects_an_in_flight_callback() {
        assert!(!session_matches(4, None));
    }

    #[test]
    fn the_unchanged_session_accepts_its_own_callback() {
        assert!(session_matches(4, Some(4)));
    }
}
