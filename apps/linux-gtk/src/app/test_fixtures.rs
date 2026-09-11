//! Model-side fixtures for the shell's tests: a session, and the objects that
//! live in one.
//!
//! Split from [`super::ui_tests`], which owns the other half — standing up a
//! real `Application` and a real window. Two responsibilities, two modules: a
//! test that only needs a `Document` inside a session has no business pulling
//! in the GTK application plumbing, and neither file has to grow every time
//! the other one does.

use super::state::{
    AnnotationAccess, ContentEditAccess, DocumentSession, PageAssemblyAccess, TextAccess,
};
use pdf_document::{Annotation, AnnotationId, AnnotationKind, Color, Document, PageId, Rect};

/// A yellow highlight on `page` — the cheapest annotation to build, and the
/// one every test that only needs *an* annotation should reach for.
///
/// Here rather than per test module for the same reason [`model_session`] is:
/// three copies of this literal is what the duplication gate flags, and none
/// of the three cares what shape the annotation has.
pub(crate) fn a_highlight(id: u64, page: PageId) -> Annotation {
    Annotation {
        id: AnnotationId(id),
        page,
        kind: AnnotationKind::Highlight {
            rect: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            color: Color {
                r: 255,
                g: 255,
                b: 0,
            },
        },
    }
}

/// A session that owns `document` and nothing else — no pdfium handle worth
/// submitting, no widgets, no caches.
///
/// `pub(crate)` and here for the same reason [`built_ui`] is: `DocumentSession`
/// has a few dozen fields, and a per-module copy of that literal turns every
/// new field into a sweep across unrelated test modules. Tests that need a
/// session to reason about the *model* start from this and set only the
/// handful of fields they are actually about.
///
/// `backend_pages` starts as the document's own page order, which is what
/// `document::show_document` installs for a freshly opened file: the handle
/// holds exactly these pages, in exactly this order.
pub(crate) fn model_session(document: Document) -> DocumentSession {
    let backend_pages = document.pages.iter().map(|page| page.id).collect();
    DocumentSession {
        // SAFETY: `DocumentHandle` wraps a `u64`. A session built here never
        // submits the handle to PDFium — tests that render capture the
        // request at the renderer boundary instead.
        document: unsafe { std::mem::zeroed() },
        base_name: "Fixture document".to_owned(),
        text_access: TextAccess::Allowed,
        annotation_access: AnnotationAccess::Allowed,
        content_edit_access: ContentEditAccess::Allowed,
        page_assembly_access: PageAssemblyAccess::Allowed,
        document_model: Some(document),
        backend_pages,
        save_backing: None,
        imported_sources: Vec::new(),
        import_warning_revision: None,
        unsaved_to_disk: false,
        edit_revision: 0,
        next_annotation_id: 0,
        selected_annotation: None,
        next_form_field_id: 0,
        selected_form_field: None,
        form_placement: None,
        form_field_drag: None,
        stamp_surfaces: Default::default(),
        placement: None,
        annotation_drag: None,
        content_editor: None,
        selected_image: None,
        image_drag: None,
        text_drag: None,
        physical_width: 800,
        physical_height: 600,
        scale_factor: 1,
        pages: Vec::new(),
        page_heights: Vec::new(),
        last_visible: None,
        search: None,
        next_search_id: 0,
        selection: None,
        active: Default::default(),
        next_render_id: 0,
        zoom: super::layout::Zoom::FitWidth,
        zoom_generation: 0,
        active_tiles: Default::default(),
    }
}
