//! What the user has *edited*, carried across a preview refresh's reopen.
//!
//! One of the two halves [`super::refresh_preview`] has to lift over
//! `document::show_document` — which resets both, correctly for its usual job
//! of installing a different document and destructively for a rebuild of the
//! one already on screen. The other half, where the user was *looking*, is
//! [`super::view`].
//!
//! A `take`/`restore` pair rather than one "preserve" call, because the lift
//! has to happen *before* `show_document` drops the session these fields
//! belong to and the put-back *after* it has installed the new one.

use std::collections::HashMap;

use gtk::cairo;
use pdf_document::Document;

use crate::app::document::backend_page_order;
use crate::app::state::{ImportedSource, SaveBacking, Viewer};

/// The half of a session that describes *what the user has edited*, as
/// opposed to what is currently being rendered.
///
/// Exists only so [`refresh_preview`](super::refresh_preview) can carry it across
/// `document::show_document`, which resets it — correctly, for its usual job of
/// installing a different document, and destructively for a preview refresh
/// of the same one.
///
/// These fields travel together because they are mutually dependent, not
/// because they happen to be convenient: `document_model` holds the
/// annotations and form fields the selections name and the id spaces their
/// counters continue — the page-id counter among them, which lives on the
/// `Document` itself and so rides across the reopen inside this one field
/// rather than needing one of its own — and its `EditLog` is keyed to exactly
/// the `save_backing` it was recorded against. Restoring any of them without the others produces
/// a session that contradicts itself — a selection pointing at nothing, ids
/// colliding with live objects, or commands replayed against a base they were
/// never validated against.
///
/// # What deliberately does *not* travel
///
/// Everything else on the session is keyed to the pdfium handle the reopen
/// replaces — a page index into *those* bytes, or text read out of them — so
/// carrying it across would mean pointing new pages at old positions. The
/// text `selection`, the `search` matches, the `selected_image` and open
/// `content_editor`, every in-flight drag, and the render/tile caches are all
/// left to die with the session `show_document` discards, and the controls
/// that read them are re-derived from the session it installs
/// (`update_search_controls` and friends). That is the invalidation rule for
/// this refresh: **a field belongs here only if its key survives the reopen**.
///
/// Two do. `AnnotationId` and `FormFieldId` outlive any handle, which is why
/// the selections below can be carried — filtered through
/// [`surviving_edit_selections`], because surviving the *reopen* is not the
/// same as surviving a page removal. `stamp_surfaces` is keyed the same way
/// and must survive for a different reason: it is a decode cache, not a
/// position, and the annotation whose bytes it holds is still in the
/// preserved model. Dropping it left every stamp the user had placed
/// painting as an empty outline (`selection::draw_annotation`'s fallback for
/// a stamp with no surface) from the next page move onward.
pub(super) struct EditState {
    document_model: Option<Document>,
    /// The refresh reopens from bytes, and `source_name` has no name for
    /// those — carrying the one the session already had is what keeps the
    /// Organize base block titled with the user's file after a page move.
    base_name: String,
    save_backing: Option<SaveBacking>,
    imported_sources: Vec<ImportedSource>,
    import_warning_revision: Option<u64>,
    next_annotation_id: u64,
    selected_annotation: Option<pdf_document::AnnotationId>,
    next_form_field_id: u64,
    selected_form_field: Option<pdf_document::FormFieldId>,
    stamp_surfaces: HashMap<pdf_document::AnnotationId, cairo::ImageSurface>,
}

fn surviving_edit_selections(
    document: Option<&Document>,
    annotation: Option<pdf_document::AnnotationId>,
    form_field: Option<pdf_document::FormFieldId>,
) -> (
    Option<pdf_document::AnnotationId>,
    Option<pdf_document::FormFieldId>,
) {
    let annotation = annotation.filter(|id| {
        document.is_some_and(|document| {
            document
                .annotations
                .get(*id)
                .is_some_and(|annotation| document.render_index(annotation.page).is_some())
        })
    });
    let form_field = form_field.filter(|id| {
        document.is_some_and(|document| {
            document
                .form_fields
                .get(*id)
                .is_some_and(|field| document.render_index(field.page).is_some())
        })
    });
    (annotation, form_field)
}

/// Lifts the edit-side state off the current session, leaving the rest of it
/// to be discarded by the `document::show_document` that follows.
///
/// Always paired with [`restore_edit_state`], which is total: it writes onto
/// whatever session is current when it runs. That matters because
/// `show_document` can bail out early (`is_current`) and leave the *old*
/// session in place — in which case the restore simply hands that session
/// back its own fields, rather than stranding it without a model.
pub(super) fn take_edit_state(viewer: &Viewer) -> Option<EditState> {
    let mut state = viewer.state.borrow_mut();
    let session = state.session.as_mut()?;
    Some(EditState {
        document_model: session.document_model.take(),
        base_name: session.base_name.clone(),
        save_backing: session.save_backing.take(),
        imported_sources: std::mem::take(&mut session.imported_sources),
        import_warning_revision: session.import_warning_revision,
        next_annotation_id: session.next_annotation_id,
        selected_annotation: session.selected_annotation,
        next_form_field_id: session.next_form_field_id,
        selected_form_field: session.selected_form_field,
        // Moved out rather than cloned: a stamp surface is a full-size
        // bitmap, and the session this is taken from is about to be dropped.
        stamp_surfaces: std::mem::take(&mut session.stamp_surfaces),
    })
}

/// Puts [`take_edit_state`]'s result back onto the session `document::show_document`
/// has just installed, and reports whether content-edit mode is still armed.
///
/// Also re-asserts `unsaved_to_disk`: the bytes just shown came from an
/// in-memory save that never touched disk, and `show_document` defaults a
/// freshly shown session to `false` — right for an ordinary open or a real
/// disk-save reopen, both of which do match disk, wrong here.
///
/// `update_annotation_controls` runs again afterwards because
/// `show_document` already ran it against the reopened model's *empty*
/// `EditLog` and left Undo/Redo greyed out; the restored model is the one
/// whose history the buttons must reflect.
pub(super) fn restore_edit_state(viewer: &Viewer, preserved: Option<EditState>) -> bool {
    let still_editing = {
        let mut state = viewer.state.borrow_mut();
        if let Some(session) = state.session.as_mut() {
            if let Some(preserved) = preserved {
                // The bytes `show_document` just installed were written from
                // this model, in this model's page order — so its ids, not
                // the reopened model's fresh 0..n, are what pdfium's pages
                // are. Set before the move, and before anything can read a
                // page index off the new session.
                let (selected_annotation, selected_form_field) = surviving_edit_selections(
                    preserved.document_model.as_ref(),
                    preserved.selected_annotation,
                    preserved.selected_form_field,
                );
                session.backend_pages = backend_page_order(preserved.document_model.as_ref());
                session.document_model = preserved.document_model;
                session.base_name = preserved.base_name;
                session.save_backing = preserved.save_backing;
                session.imported_sources = preserved.imported_sources;
                session.import_warning_revision = preserved.import_warning_revision;
                session.next_annotation_id = preserved.next_annotation_id;
                session.selected_annotation = selected_annotation;
                session.next_form_field_id = preserved.next_form_field_id;
                session.selected_form_field = selected_form_field;
                session.stamp_surfaces = preserved.stamp_surfaces;
            }
            session.unsaved_to_disk = true;
        }
        state.content_edit_mode
    };
    crate::app::annotations::update_annotation_controls(viewer);
    crate::app::forms::update_forms_controls(viewer);
    still_editing
}

#[cfg(test)]
mod tests {
    use super::{restore_edit_state, surviving_edit_selections, take_edit_state};
    use crate::app::test_fixtures::{a_form_field, a_highlight, model_session};
    use crate::app::ui_tests::built_ui;
    use pdf_document::{AnnotationId, Document, FormFieldId, Orientation, Page, PageId, PageSize};

    #[test]
    fn edit_selections_survive_when_the_preserved_model_still_holds_them() {
        let mut document = Document::blank();
        document
            .pages
            .push(Page::blank(PageId(0), PageSize::A4, Orientation::Portrait));
        document.annotations.insert(a_highlight(4, PageId(0)));
        document.form_fields.insert(a_form_field(7));

        assert_eq!(
            surviving_edit_selections(Some(&document), Some(AnnotationId(4)), Some(FormFieldId(7)),),
            (Some(AnnotationId(4)), Some(FormFieldId(7)))
        );
    }

    #[test]
    fn edit_selections_are_cleared_when_their_page_was_removed() {
        let mut document = Document::blank();
        document.annotations.insert(a_highlight(4, PageId(0)));
        document.form_fields.insert(a_form_field(7));

        assert_eq!(
            surviving_edit_selections(Some(&document), Some(AnnotationId(4)), Some(FormFieldId(7)),),
            (None, None)
        );
    }

    #[test]
    fn edit_selections_are_cleared_when_the_preserved_model_no_longer_holds_them() {
        assert_eq!(
            surviving_edit_selections(
                Some(&Document::blank()),
                Some(AnnotationId(4)),
                Some(FormFieldId(7)),
            ),
            (None, None)
        );
    }

    #[test]
    fn edit_selections_are_cleared_without_an_editable_model() {
        assert_eq!(
            surviving_edit_selections(None, Some(AnnotationId(4)), Some(FormFieldId(7))),
            (None, None)
        );
    }

    /// The invalidation rule [`EditState`] documents, exercised end to
    /// end across the reopen: what is keyed to the replaced pdfium handle is
    /// dropped, what is keyed to an id that outlives it is carried.
    #[gtk::test]
    fn gtk_ui_a_preview_refresh_keeps_id_keyed_caches_and_drops_backend_keyed_state() {
        use crate::app::state::{SearchState, Selection};
        use gtk::cairo;
        use gtk::prelude::GtkWindowExt;

        let built = built_ui();
        let mut document = Document::blank();
        document
            .pages
            .push(Page::blank(PageId(0), PageSize::A4, Orientation::Portrait));
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
            .expect("a 1x1 surface is always creatable");

        {
            let mut state = built.viewer.state.borrow_mut();
            let mut session = model_session(document.clone());
            session.stamp_surfaces.insert(AnnotationId(3), surface);
            session.selection = Some(Selection {
                page_index: 0,
                anchor: (0.0, 0.0),
                focus: (1.0, 1.0),
            });
            session.search = Some(SearchState {
                query: "find me".to_string(),
                matches: Vec::new(),
                current: 0,
            });
            state.session = Some(session);
        }

        let preserved = take_edit_state(&built.viewer);
        // What `show_document` does to the session on its way through: a new
        // one, built from the bytes that were just reopened.
        built.viewer.state.borrow_mut().session = Some(model_session(document));
        restore_edit_state(&built.viewer, preserved);

        {
            let state = built.viewer.state.borrow();
            let session = state.session.as_ref().expect("a session was installed");
            assert!(
                session.stamp_surfaces.contains_key(&AnnotationId(3)),
                "a stamp's decoded bitmap is keyed by an id the reopen does not change"
            );
            assert!(
                session.selection.is_none(),
                "a text selection names a backend page position and must not survive"
            );
            assert!(
                session.search.is_none(),
                "search matches name backend page positions and must not survive"
            );
        }

        built.viewer.state.borrow_mut().session = None;
        built.window.close();
    }
}
