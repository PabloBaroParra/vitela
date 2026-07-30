//! Shared data types for the shell: the widget handle bundle, per-document
//! session state, and the small value types the feature modules pass around.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{Box as GtkBox, Button, Dialog, Entry, Label, Overlay, Picture, ScrolledWindow};
use pdf_render::{CancellationHandle, DocumentHandle, TextMatch};

/// Where an open request's bytes come from.
///
/// The password-retry loop re-opens the *same* source, so this has to be
/// cheap to clone and outlive the first attempt — hence an owned path rather
/// than a borrow, and a `'static` slice for the compiled-in sample.
#[derive(Clone)]
pub(crate) enum DocumentSource {
    /// A file the user picked. GTK4 is the one shell that can hand pdfium a
    /// real filesystem path (see `PdfiumRenderer::open_document`).
    File(PathBuf),
    /// The sample document baked into the binary at compile time.
    Embedded(&'static [u8]),
}

#[derive(Clone)]
pub(crate) struct Viewer {
    pub(crate) scroll: ScrolledWindow,
    pub(crate) pages: GtkBox,
    /// Brand mark overlaid on the page area. Visible exactly while there is
    /// nothing to show — see `brand::build_app_mark`.
    pub(crate) app_mark: Picture,
    pub(crate) status: Label,
    pub(crate) search_entry: Entry,
    pub(crate) find_previous: Button,
    pub(crate) find_next: Button,
    pub(crate) print_button: Button,
    pub(crate) state: Rc<RefCell<ViewerState>>,
}

pub(crate) struct ViewerState {
    pub(crate) generation: u64,
    pub(crate) session: Option<DocumentSession>,
    /// The password prompt for the in-flight open attempt, if any. Tracked so
    /// a new open request (which may supersede this one before the user has
    /// answered) can tear down the stale prompt instead of leaving it
    /// stacked underneath a second one — see `document::begin_loading` and
    /// `document::dismiss_password_dialog`.
    pub(crate) password_dialog: Option<Dialog>,
}

pub(crate) struct DocumentSession {
    pub(crate) document: DocumentHandle,
    pub(crate) physical_width: u32,
    pub(crate) physical_height: u32,
    pub(crate) scale_factor: i32,
    pub(crate) pages: Vec<PageSlot>,
    /// Cached logical heights, one per page — recomputed only when the fit
    /// changes (`show_document`/`refresh_layout`) so the per-scroll
    /// `update_viewport` never re-queries every widget's size request.
    pub(crate) page_heights: Vec<i32>,
    /// The last `(first, last)` range reported to the status label, so a
    /// scroll tick that doesn't move the visible range skips the redundant
    /// `format!` + `set_text`.
    pub(crate) last_visible: Option<(usize, usize)>,
    /// Matches for the last query run against *this* document. Lives in the
    /// session so replacing the document drops them with it.
    pub(crate) search: Option<SearchState>,
    /// Id of the most recently issued search. A slow search whose id is no
    /// longer current has been superseded by a later query and must not
    /// overwrite its results.
    pub(crate) next_search_id: u64,
    pub(crate) active: HashMap<usize, ActiveRender>,
    pub(crate) next_render_id: u64,
    pub(crate) zoom: super::layout::Zoom,
    pub(crate) zoom_generation: u64,
    pub(crate) active_tiles: HashMap<usize, ActiveRender>,
}

pub(crate) struct SearchState {
    pub(crate) query: String,
    pub(crate) matches: Vec<TextMatch>,
    pub(crate) current: usize,
}

pub(crate) struct ActiveRender {
    pub(crate) id: u64,
    pub(crate) generation: u64,
    pub(crate) cancellation: CancellationHandle,
}

pub(crate) struct PageSlot {
    pub(crate) overlay: Overlay,
    pub(crate) picture: Picture,
    pub(crate) width_pt: f32,
    pub(crate) height_pt: f32,
    pub(crate) state: PageState,
    pub(crate) target_dpi: u32,
    pub(crate) budget: super::layout::TileBudget,
    pub(crate) tiles: HashMap<super::layout::TileRect, Picture>,
    pub(crate) tile_dpi: u32,
    pub(crate) tile_generation: u64,
    /// DPI whose tile batch failed, or 0. Terminal for that DPI so a doomed
    /// batch isn't re-queued on every scroll tick; a new zoom clears it.
    pub(crate) tile_failed_dpi: u32,
}

/// Render lifecycle of a single page slot. `Skipped`/`Failed` are terminal
/// for the current fit: they keep `update_viewport` from re-queuing a job
/// that can only be rejected or fail again. A new fit (`refresh_layout`)
/// resets every slot to `Idle`, giving oversized/failed pages one retry at
/// the new size.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum PageState {
    Idle,
    Rendered,
    Skipped,
    Failed,
}

/// A rendered page in `Send` form: produced on a worker thread, converted
/// into a non-`Send` pixbuf only on the GTK main thread.
pub(crate) struct RenderedPage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) pixels: Vec<u8>,
}

pub(crate) struct OpenedDocument {
    pub(crate) document: DocumentHandle,
    pub(crate) page_sizes: Vec<(f32, f32)>,
}

#[derive(Clone, Copy)]
pub(crate) struct FitRequest {
    pub(crate) available_width: u32,
    pub(crate) available_height: u32,
    pub(crate) scale_factor: i32,
}

impl FitRequest {
    pub(crate) fn measure(viewer: &Viewer) -> Self {
        let scale_factor = viewer.scroll.scale_factor().max(1);
        FitRequest {
            available_width: (viewer.scroll.width().max(1) * scale_factor) as u32,
            available_height: (viewer.scroll.height().max(1) * scale_factor) as u32,
            scale_factor,
        }
    }
}
