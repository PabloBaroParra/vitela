//! What each [`Target`] runs and whether it can run now.
//!
//! Every arm calls the function the matching button or accelerator already
//! calls; nothing here decides a permission. Refusal text comes from the
//! `*_refusal` gates, and the "no document" text from the message the
//! feature itself shows when invoked without one.

use gtk::prelude::*;
use gtk::ApplicationWindow;

use super::{Availability, ClickTool, Target};
use crate::app::home::{self, tools::open_tool};
use crate::app::i18n::tr;
use crate::app::mode_bar::Mode;
use crate::app::organize::ORGANIZE_PAGE;
use crate::app::shortcuts::{self, Shortcut};
use crate::app::state::{FieldKind, HomeTool, Tool, Viewer};
use crate::app::tools_panel::{ANNOTATE_PAGE, FILL_SIGN_PAGE};

pub(super) fn availability(target: Target, viewer: &Viewer) -> Availability {
    let has_document = viewer.state.borrow().session.is_some();
    let refusal = match target {
        Target::Mode(mode) => mode_refusal(mode, viewer, has_document),
        Target::Action(shortcut) => action_refusal(shortcut, viewer, has_document),
        Target::Annotation(_) => document_then(has_document, "annotate-no-document", || {
            viewer.annotation_editing_refusal()
        }),
        Target::FormField(_) => document_then(has_document, "forms-no-document", || {
            crate::app::forms::structural_edit_refusal(viewer)
        }),
        Target::Click(tool) => click_refusal(tool, viewer, has_document),
    };
    match refusal {
        Some(reason) => Availability::Refused(reason),
        None => Availability::Available,
    }
}

pub(super) fn run(target: Target, window: &ApplicationWindow, viewer: &Viewer) {
    match target {
        Target::Mode(mode) => mode.open(window, viewer),
        Target::Action(shortcut) => {
            ActionGroupExt::activate_action(window, shortcut.action, None);
        }
        Target::Annotation(tool) => arm_annotation(viewer, tool),
        Target::FormField(kind) => arm_form_field(viewer, kind),
        Target::Click(tool) => run_click(tool, window, viewer),
    }
}

/// With no document, `no_document`'s message; otherwise whatever `gate` says.
fn document_then(
    has_document: bool,
    no_document: &str,
    gate: impl FnOnce() -> Option<String>,
) -> Option<String> {
    if has_document {
        gate()
    } else {
        Some(tr(no_document))
    }
}

/// Modes are never refused for a missing document: `open_tool` picks one
/// first (decision 3's exception). With a document, the two modes whose
/// door refuses report that refusal; the others are navigation, and the
/// page they land on states its own.
fn mode_refusal(mode: Mode, viewer: &Viewer, has_document: bool) -> Option<String> {
    if !has_document {
        return None;
    }
    match mode {
        Mode::Organize => viewer.content_edit_refusal(),
        Mode::Protect => crate::app::protect::protection_refusal(viewer),
        Mode::Home | Mode::Edit | Mode::Annotate | Mode::Sign => None,
    }
}

fn action_refusal(shortcut: Shortcut, viewer: &Viewer, has_document: bool) -> Option<String> {
    match shortcut {
        shortcuts::OPEN | shortcuts::NEW => None,
        shortcuts::SAVE => document_then(has_document, "save-no-document", || None),
        shortcuts::PRINT => document_then(has_document, "print-open-first", || None),
        shortcuts::FIND => document_then(has_document, "search-open-first", || {
            viewer.text_extraction_refusal()
        }),
        shortcuts::UNDO => {
            (!viewer.undo_action.is_enabled()).then(|| tr("palette-nothing-to-undo"))
        }
        shortcuts::REDO => {
            (!viewer.redo_action.is_enabled()).then(|| tr("palette-nothing-to-redo"))
        }
        other => unreachable!("no palette entry for win.{}", other.action),
    }
}

fn click_refusal(tool: ClickTool, viewer: &Viewer, has_document: bool) -> Option<String> {
    match tool {
        ClickTool::ExportImages => document_then(has_document, "export-no-document", || {
            viewer.text_extraction_refusal()
        }),
        ClickTool::Watermark => document_then(has_document, "watermark-no-document", || {
            viewer.content_edit_refusal()
        }),
        // A Home tool: from a cold start the door picks a file first, and
        // `begin_compress` reports its own refusals — it needs the whole
        // save snapshot to know them, which is too much to build per search.
        ClickTool::Compress => None,
        ClickTool::AddPdfs => document_then(has_document, "organize-no-document", || {
            crate::app::organize::import_refusal(viewer)
        }),
        ClickTool::Extract => document_then(has_document, "extract-no-document", || {
            extraction_refusal(viewer)
        }),
        ClickTool::Split => document_then(has_document, "split-no-document", || {
            extraction_refusal(viewer)
        }),
        ClickTool::SignWithFile
        | ClickTool::SignWithToken
        | ClickTool::SignWithSystemCertificate => {
            document_then(has_document, "sign-no-document", || {
                crate::app::sign::signing_refusal(viewer)
            })
        }
        ClickTool::DrawSignature => document_then(has_document, "signature-no-document", || {
            viewer.annotation_editing_refusal()
        }),
    }
}

/// The two gates `begin_extract` and `begin_split` check, in their order.
fn extraction_refusal(viewer: &Viewer) -> Option<String> {
    viewer
        .text_extraction_refusal()
        .or_else(|| viewer.full_rewrite_refusal())
}

/// Shows the editor with `page` up on the tools panel.
fn show_tools_page(viewer: &Viewer, page: &str) {
    home::show_editor(viewer);
    viewer.tools_stack.set_visible_child_name(page);
}

/// Switches the tool's toggle on, which runs the toggle's own handler —
/// exactly what a click on it does. Already on stays on.
fn arm_annotation(viewer: &Viewer, tool: Tool) {
    show_tools_page(viewer, ANNOTATE_PAGE);
    if let Some((_, button)) = viewer
        .annotation_buttons
        .create
        .iter()
        .find(|(candidate, _)| *candidate == tool)
    {
        button.set_active(true);
    }
}

/// The form-field twin of [`arm_annotation`], on the Fill & Sign page.
fn arm_form_field(viewer: &Viewer, kind: FieldKind) {
    show_tools_page(viewer, FILL_SIGN_PAGE);
    if let Some((_, button)) = viewer
        .forms
        .place
        .iter()
        .find(|(candidate, _)| *candidate == kind)
    {
        button.set_active(true);
    }
}

fn run_click(tool: ClickTool, window: &ApplicationWindow, viewer: &Viewer) {
    match tool {
        ClickTool::ExportImages => crate::app::export::begin_export(viewer),
        ClickTool::Watermark => crate::app::content_edit::watermark::begin_watermark(viewer),
        ClickTool::Compress => open_tool(window, viewer, HomeTool::Compress),
        // Add PDFs reports its progress, and offers its cancel, in the
        // Organize header — so the import starts from Organize, as the
        // button's does. `organize::show` refuses on its own terms, and then
        // there is no import to start.
        ClickTool::AddPdfs => {
            if viewer.view_stack.visible_child_name().as_deref() != Some(ORGANIZE_PAGE) {
                crate::app::organize::show(viewer);
            }
            if viewer.view_stack.visible_child_name().as_deref() == Some(ORGANIZE_PAGE) {
                crate::app::organize::show_import_chooser(window, viewer);
            }
        }
        ClickTool::Extract => crate::app::write::begin_extract(window, viewer),
        ClickTool::Split => crate::app::write::begin_split(window, viewer),
        // The signing buttons live on Fill & Sign, and a drawn signature is
        // released as soon as that page is left (`signature::connect_signature`),
        // so all four start from there.
        ClickTool::SignWithFile => {
            show_tools_page(viewer, FILL_SIGN_PAGE);
            crate::app::sign::show_pfx_chooser(window, viewer);
        }
        ClickTool::SignWithToken => {
            show_tools_page(viewer, FILL_SIGN_PAGE);
            crate::app::sign::begin_pkcs11_flow(window, viewer);
        }
        ClickTool::SignWithSystemCertificate => {
            show_tools_page(viewer, FILL_SIGN_PAGE);
            crate::app::sign::begin_nss_flow(window, viewer);
        }
        ClickTool::DrawSignature => {
            show_tools_page(viewer, FILL_SIGN_PAGE);
            crate::app::signature::draw_signature(viewer);
        }
    }
}
