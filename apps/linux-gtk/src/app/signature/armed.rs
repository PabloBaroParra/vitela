//! A drawn signature waiting for its page: arming it, releasing it, and the
//! page click that places it.
//!
//! "Armed" is the same idea as an armed annotation tool, and the landing is the
//! same code: the click goes to `annotations::stamp_from_image_bytes`, the path
//! Ctrl+V and a dropped image file already use. That path asks the core's stamp
//! placement for the size, so a wide signature keeps its proportions instead of
//! being squashed into a click-sized box, and it is one undo step.

use crate::app::state::{ArmedSignature, Viewer};
use crate::app::{annotations, content_edit, forms};

use super::current_session_id;

const PROMPT: &str = "Click a page to place your signature.";
const DISARMED: &str = "Signature disarmed.";
const STALE: &str = "That signature was drawn for another document. Draw it again to place it.";

/// Arms `png` so the next page click places it, after releasing whatever else
/// would take that click.
///
/// `session_id` names the document the signature was drawn for. A PNG that
/// finishes rendering after another document replaced it arms nothing, and the
/// answer says so — the caller must not announce an armed signature that is not
/// there.
pub(super) fn arm(viewer: &Viewer, session_id: u64, png: Vec<u8>) -> bool {
    if current_session_id(viewer) != Some(session_id) {
        return false;
    }
    // One mode claims a page click at a time. Choosing a tool, content-edit or
    // forms-edit mode releases a waiting signature through `annotations::disarm`
    // and `arm_tool`; this is the other direction of the same exclusion.
    content_edit::set_mode(viewer, false);
    forms::set_mode(viewer, false);
    annotations::disarm(viewer);
    viewer.state.borrow_mut().signature.armed = Some(ArmedSignature { png, session_id });
    viewer.status.set_text(PROMPT);
    true
}

/// Releases a waiting signature, if there is one. Leaving the Sign tab calls
/// this: nothing visible says a click is about to place a picture once the user
/// is looking at another tool's panel.
pub(super) fn disarm(viewer: &Viewer) {
    let was_armed = viewer.state.borrow_mut().signature.armed.take().is_some();
    if was_armed {
        viewer.status.set_text(DISARMED);
    }
}

/// What a page press means to a waiting signature.
enum Press {
    /// Nothing is waiting: the press is somebody else's.
    NotArmed,
    /// The signature was drawn for a document that is gone, and was dropped.
    Stale,
    /// The document does not permit it; the signature stays armed.
    Refused(&'static str),
    /// Place this picture.
    Place(Vec<u8>),
}

/// The page press that places the armed signature. Returns whether the press
/// was claimed — a `false` leaves it to the next handler, as an unarmed tool
/// does.
///
/// A refusal (the document does not permit annotation) keeps the signature
/// armed and still claims the press: the user pointed at the page to place
/// something, and being told why it did not land beats starting a text
/// selection under them.
pub(crate) fn place_armed(viewer: &Viewer, page_index: usize, point: (f64, f64)) -> bool {
    let press = take_press(viewer);
    match press {
        Press::NotArmed => false,
        Press::Stale => {
            viewer.status.set_text(STALE);
            false
        }
        Press::Refused(refusal) => {
            viewer.status.set_text(refusal);
            true
        }
        Press::Place(png) => {
            annotations::stamp_from_image_bytes(viewer, page_index, point, png);
            true
        }
    }
}

/// Decides what the press means and, when it places, takes the picture out of
/// the state — in one borrow, so nothing can arm or disarm in between.
fn take_press(viewer: &Viewer) -> Press {
    let current = current_session_id(viewer);
    let mut state = viewer.state.borrow_mut();
    let armed_for = state.signature.armed.as_ref().map(|armed| armed.session_id);
    match armed_for {
        None => Press::NotArmed,
        Some(session_id) if Some(session_id) != current => {
            state.signature.armed = None;
            Press::Stale
        }
        Some(_) => {
            let refusal = state
                .session
                .as_ref()
                .and_then(|session| session.annotation_access.refusal());
            if let Some(refusal) = refusal {
                return Press::Refused(refusal);
            }
            // The signature replaces whatever was selected: a drag that
            // started before it must not go on extending a text selection
            // under it.
            if let Some(session) = state.session.as_mut() {
                session.selection = None;
            }
            match state.signature.armed.take() {
                Some(armed) => Press::Place(armed.png),
                None => Press::NotArmed,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use gtk::prelude::*;
    use pdf_document::AnnotationKind;

    use crate::app::state::{AnnotationAccess, Tool};
    use crate::app::tools_panel::{ANNOTATE_PAGE, FILL_SIGN_PAGE};

    use super::super::test_support::{a_png, is_armed, open_document, the_session, Built};

    fn stamps(viewer: &Viewer) -> usize {
        viewer
            .state
            .borrow()
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref())
            .map_or(0, |document| {
                document
                    .annotations
                    .iter()
                    .filter(|annotation| matches!(annotation.kind, AnnotationKind::Stamp { .. }))
                    .count()
            })
    }

    fn tool_button(viewer: &Viewer, tool: Tool) -> gtk::ToggleButton {
        viewer
            .annotation_buttons
            .create
            .iter()
            .find(|(candidate, _)| *candidate == tool)
            .map(|(_, button)| button.clone())
            .expect("every tool has a button")
    }

    #[gtk::test]
    fn gtk_ui_the_next_page_press_places_the_armed_signature_as_one_stamp() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        assert!(arm(viewer, the_session(viewer), a_png()));

        let claimed = place_armed(viewer, 0, (100.0, 700.0));

        assert!(claimed, "the press must not fall through to text selection");
        assert_eq!(stamps(viewer), 1);
        assert!(!is_armed(viewer), "one signature, one placement");
    }

    #[gtk::test]
    fn gtk_ui_a_placed_signature_keeps_its_own_proportions() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        assert!(arm(viewer, the_session(viewer), a_png()));

        place_armed(viewer, 0, (100.0, 700.0));

        let state = viewer.state.borrow();
        let document = state
            .session
            .as_ref()
            .and_then(|session| session.document_model.as_ref())
            .expect("a model");
        let Some(AnnotationKind::Stamp { rect, .. }) =
            document.annotations.iter().map(|a| &a.kind).next()
        else {
            panic!("a stamp was placed");
        };
        // The picture is 123 x 43 px (120 x 40 of ink plus a 3 px stroke), so
        // a square click-sized box would be wrong.
        let picture_ratio = 123.0 / 43.0;
        assert!(((rect.width / rect.height) - picture_ratio).abs() < 0.05);
    }

    #[gtk::test]
    fn gtk_ui_a_press_with_nothing_armed_is_left_to_the_next_handler() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);

        assert!(!place_armed(viewer, 0, (100.0, 700.0)));
        assert_eq!(stamps(viewer), 0);
    }

    #[gtk::test]
    fn gtk_ui_a_signature_armed_for_another_document_places_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let drawn_over = the_session(viewer);
        assert!(arm(viewer, drawn_over, a_png()));
        open_document(viewer);

        assert!(!place_armed(viewer, 0, (100.0, 700.0)));

        assert_eq!(stamps(viewer), 0);
        assert!(!is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_a_picture_that_arrives_after_another_document_opened_arms_nothing() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let drawn_over = the_session(viewer);
        open_document(viewer);

        assert!(!arm(viewer, drawn_over, a_png()));

        assert!(!is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_a_refused_placement_keeps_the_signature_armed() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        assert!(arm(viewer, the_session(viewer), a_png()));
        if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
            session.annotation_access = AnnotationAccess::Forbidden;
        }

        assert!(place_armed(viewer, 0, (100.0, 700.0)));

        assert_eq!(stamps(viewer), 0);
        assert!(is_armed(viewer), "a refusal is not a placement");
        assert_eq!(
            viewer.status.text(),
            "This document does not permit annotation changes."
        );
    }

    #[gtk::test]
    fn gtk_ui_arming_releases_an_armed_annotation_tool() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        let stamp_tool = tool_button(viewer, Tool::Stamp);
        stamp_tool.set_active(true);
        assert_eq!(viewer.state.borrow().active_tool, Some(Tool::Stamp));

        assert!(arm(viewer, the_session(viewer), a_png()));

        assert_eq!(viewer.state.borrow().active_tool, None);
        assert!(!stamp_tool.is_active());
        assert!(is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_choosing_a_tool_releases_an_armed_signature() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        assert!(arm(viewer, the_session(viewer), a_png()));

        tool_button(viewer, Tool::Stamp).set_active(true);

        assert!(!is_armed(viewer));
        assert_eq!(viewer.state.borrow().active_tool, Some(Tool::Stamp));
    }

    #[gtk::test]
    fn gtk_ui_arming_releases_forms_edit_mode() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        forms::set_mode(viewer, true);

        assert!(arm(viewer, the_session(viewer), a_png()));

        assert!(!viewer.state.borrow().form_edit_mode);
    }

    #[gtk::test]
    fn gtk_ui_entering_forms_edit_mode_releases_an_armed_signature() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        assert!(arm(viewer, the_session(viewer), a_png()));

        forms::set_mode(viewer, true);

        assert!(!is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_leaving_the_sign_tab_releases_an_armed_signature() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        viewer.tools_stack.set_visible_child_name(FILL_SIGN_PAGE);
        assert!(arm(viewer, the_session(viewer), a_png()));

        viewer.tools_stack.set_visible_child_name(ANNOTATE_PAGE);

        assert!(!is_armed(viewer));
    }

    #[gtk::test]
    fn gtk_ui_staying_on_the_sign_tab_keeps_the_signature_armed() {
        let built = Built::new();
        let viewer = built.viewer();
        open_document(viewer);
        viewer.tools_stack.set_visible_child_name(ANNOTATE_PAGE);
        viewer.tools_stack.set_visible_child_name(FILL_SIGN_PAGE);

        assert!(arm(viewer, the_session(viewer), a_png()));

        assert!(is_armed(viewer));
    }
}
