use std::collections::HashSet;

use gtk::prelude::*;
use pdf_document::{Document, Orientation, Page, PageId, PageSize, Rotation};

use super::*;
use crate::app::home::{EDITOR_PAGE, HOME_PAGE};
use crate::app::i18n::{has_message, speak};
use crate::app::state::AnnotationAccess;
use crate::app::test_fixtures::model_session;
use crate::app::tools_panel::{ANNOTATE_PAGE, FILL_SIGN_PAGE};
use crate::app::ui_tests::built_ui;
use crate::app::BuiltUi;

/// Closes the window when the test ends, panicking or not, with the session
/// cleared first so closing raises no unsaved-changes prompt.
struct Window(BuiltUi);

impl Drop for Window {
    fn drop(&mut self) {
        self.0.viewer.state.borrow_mut().session = None;
        self.0.window.close();
    }
}

fn window() -> Window {
    Window(built_ui())
}

/// A one-page document installed without pdfium: availability only asks the
/// session's access fields.
fn open_document(viewer: &Viewer) {
    let session = model_session(Document::with_pages(vec![Page::base(
        PageId(0),
        0,
        PageSize::A4,
        Orientation::Portrait,
        Rotation::None,
    )]));
    viewer.state.borrow_mut().session = Some(session);
}

fn entry(target: Target) -> PaletteEntry {
    let matching: Vec<PaletteEntry> = catalog()
        .into_iter()
        .filter(|entry| entry.target == target)
        .collect();
    assert_eq!(matching.len(), 1, "{target:?} must have exactly one entry");
    matching[0]
}

fn found_ids(viewer: &Viewer, query: &str) -> Vec<&'static str> {
    search(viewer, query)
        .into_iter()
        .map(|(entry, _, _)| entry.id)
        .collect()
}

#[test]
fn every_mode_has_exactly_one_entry() {
    for mode in Mode::ALL {
        entry(Target::Mode(mode));
    }
}

#[test]
fn every_annotation_tool_has_exactly_one_entry() {
    for tool in Tool::ALL {
        entry(Target::Annotation(tool));
    }
}

#[test]
fn every_form_field_kind_has_exactly_one_entry() {
    for kind in FieldKind::ALL {
        entry(Target::FormField(kind));
    }
}

#[test]
fn every_click_tool_has_exactly_one_entry() {
    for tool in ClickTool::ALL {
        entry(Target::Click(tool));
    }
}

#[test]
fn ids_are_unique() {
    let entries = catalog();
    let ids: HashSet<&str> = entries.iter().map(|entry| entry.id).collect();
    assert_eq!(ids.len(), entries.len());
}

#[test]
fn every_label_resolves_in_english_and_spanish() {
    for entry in catalog() {
        for language in [Language::English, Language::Spanish] {
            assert!(
                has_message(language, entry.label_key),
                "{} has no {language:?} label {:?}",
                entry.id,
                entry.label_key
            );
        }
    }
}

#[test]
fn action_entries_show_the_accelerator_gtk_registers() {
    for (target, shortcut) in [
        (Target::Action(WindowAction::Open), shortcuts::OPEN),
        (Target::Action(WindowAction::Undo), shortcuts::UNDO),
        (Target::Action(WindowAction::Find), shortcuts::FIND),
    ] {
        assert_eq!(entry(target).shortcut, Some(shortcut.accel));
    }
}

#[test]
fn categories_are_contiguous() {
    let entries = catalog();
    let mut seen: Vec<Category> = Vec::new();
    for entry in &entries {
        if seen.last() != Some(&entry.category) {
            assert!(
                !seen.contains(&entry.category),
                "{:?} is split in two",
                entry.category
            );
            seen.push(entry.category);
        }
    }
}

#[gtk::test]
fn gtk_ui_an_empty_query_keeps_catalog_order() {
    let built = window();
    let ids: Vec<&str> = catalog().iter().map(|entry| entry.id).collect();
    assert_eq!(found_ids(&built.0.viewer, ""), ids);
}

#[gtk::test]
fn gtk_ui_spanish_without_accents_finds_its_commands() {
    let built = window();
    let _spanish = speak(Language::Spanish);
    // "Añadir PDF" and "Exportar imágenes", typed without their accents.
    assert_eq!(
        found_ids(&built.0.viewer, "anadir pdf").first(),
        Some(&"pages.add-pdfs")
    );
    assert_eq!(
        found_ids(&built.0.viewer, "imagenes").first(),
        Some(&"document.export-images")
    );
}

#[gtk::test]
fn gtk_ui_the_english_label_matches_while_speaking_spanish() {
    let built = window();
    let _spanish = speak(Language::Spanish);
    assert_eq!(
        found_ids(&built.0.viewer, "highlight").first(),
        Some(&"annotate.highlight")
    );
    assert_eq!(
        found_ids(&built.0.viewer, "watermark").first(),
        Some(&"document.watermark")
    );
}

#[gtk::test]
fn gtk_ui_an_annotation_refusal_reaches_every_annotation_entry() {
    let built = window();
    let viewer = &built.0.viewer;
    open_document(viewer);
    if let Some(session) = viewer.state.borrow_mut().session.as_mut() {
        session.annotation_access = AnnotationAccess::Forbidden;
    }
    let refusal = viewer
        .annotation_editing_refusal()
        .expect("the fixture forbids annotation");

    for tool in Tool::ALL {
        assert_eq!(
            entry(Target::Annotation(tool)).availability(viewer),
            Availability::Refused(refusal.clone()),
            "{tool:?}"
        );
    }
    assert_eq!(
        entry(Target::Click(ClickTool::DrawSignature)).availability(viewer),
        Availability::Refused(refusal)
    );
    // Navigation is never refused: the Annotate page states the refusal.
    assert_eq!(
        entry(Target::Mode(Mode::Annotate)).availability(viewer),
        Availability::Available
    );
}

#[gtk::test]
fn gtk_ui_an_allowed_document_makes_annotation_entries_available() {
    let built = window();
    let viewer = &built.0.viewer;
    open_document(viewer);
    for tool in Tool::ALL {
        assert_eq!(
            entry(Target::Annotation(tool)).availability(viewer),
            Availability::Available,
            "{tool:?}"
        );
    }
}

#[gtk::test]
fn gtk_ui_without_a_document_only_modes_and_document_free_commands_run() {
    let built = window();
    let viewer = &built.0.viewer;

    for mode in Mode::ALL {
        assert_eq!(
            entry(Target::Mode(mode)).availability(viewer),
            Availability::Available,
            "{mode:?} must stay available: open_tool picks a file first"
        );
    }
    for action in [WindowAction::Open, WindowAction::New] {
        assert_eq!(
            entry(Target::Action(action)).availability(viewer),
            Availability::Available
        );
    }
    assert_eq!(
        entry(Target::Click(ClickTool::Compress)).availability(viewer),
        Availability::Available
    );
    for (target, key) in [
        (Target::Action(WindowAction::Save), "save-no-document"),
        (Target::Action(WindowAction::Print), "print-open-first"),
        (Target::Action(WindowAction::Find), "search-open-first"),
        (Target::Annotation(Tool::Highlight), "annotate-no-document"),
        (Target::FormField(FieldKind::Text), "forms-no-document"),
        (Target::Click(ClickTool::ExportImages), "export-no-document"),
        (Target::Click(ClickTool::AddPdfs), "organize-no-document"),
        (Target::Click(ClickTool::SignWithFile), "sign-no-document"),
    ] {
        assert_eq!(
            entry(target).availability(viewer),
            Availability::Refused(tr(key)),
            "{target:?}"
        );
    }
}

#[gtk::test]
fn gtk_ui_undo_follows_the_window_action() {
    let built = window();
    let viewer = &built.0.viewer;
    open_document(viewer);
    viewer.undo_action.set_enabled(false);
    assert_eq!(
        entry(Target::Action(WindowAction::Undo)).availability(viewer),
        Availability::Refused(tr("palette-nothing-to-undo"))
    );
    viewer.undo_action.set_enabled(true);
    assert_eq!(
        entry(Target::Action(WindowAction::Undo)).availability(viewer),
        Availability::Available
    );
}

fn screen(viewer: &Viewer) -> (Option<String>, Option<String>) {
    (
        viewer.view_stack.visible_child_name().map(Into::into),
        viewer.tools_stack.visible_child_name().map(Into::into),
    )
}

/// The palette's mode entry and the mode's button land on the same screen.
#[gtk::test]
fn gtk_ui_running_a_mode_entry_lands_where_its_button_does() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);

    for mode in [Mode::Edit, Mode::Annotate, Mode::Sign, Mode::Home] {
        ui.viewer.view_stack.set_visible_child_name(HOME_PAGE);
        ui.mode_bar.button(mode).emit_clicked();
        let clicked = screen(&ui.viewer);

        ui.viewer.view_stack.set_visible_child_name(HOME_PAGE);
        entry(Target::Mode(mode))
            .run(&ui.window, &ui.viewer)
            .expect("a mode is never refused");

        assert_eq!(screen(&ui.viewer), clicked, "{mode:?}");
    }
}

/// An annotation entry arms the same tool its toggle does, on the Annotate
/// page.
#[gtk::test]
fn gtk_ui_running_an_annotation_entry_arms_its_tool() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    crate::app::annotations::update_annotation_controls(&ui.viewer);

    entry(Target::Annotation(Tool::Ink))
        .run(&ui.window, &ui.viewer)
        .expect("annotation is allowed");

    assert_eq!(ui.viewer.state.borrow().active_tool, Some(Tool::Ink));
    assert_eq!(
        screen(&ui.viewer),
        (Some(EDITOR_PAGE.into()), Some(ANNOTATE_PAGE.into()))
    );
}

/// A form-field entry arms the same kind its toggle does, on Fill & Sign.
#[gtk::test]
fn gtk_ui_running_a_form_field_entry_arms_its_kind() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    crate::app::forms::update_forms_controls(&ui.viewer);

    entry(Target::FormField(FieldKind::Checkbox))
        .run(&ui.window, &ui.viewer)
        .expect("form fields are allowed");

    assert_eq!(
        ui.viewer.state.borrow().form_field_kind,
        Some(FieldKind::Checkbox)
    );
    assert_eq!(
        screen(&ui.viewer),
        (Some(EDITOR_PAGE.into()), Some(FILL_SIGN_PAGE.into()))
    );
}

/// A refused entry does not run (T-229 review): the annotation toggle's
/// handler arms its tool even while the button is insensitive, so the
/// entry's own run path has to say no first.
#[gtk::test]
fn gtk_ui_running_a_refused_annotation_entry_leaves_the_tool_unarmed() {
    let built = window();
    let ui = &built.0;
    open_document(&ui.viewer);
    if let Some(session) = ui.viewer.state.borrow_mut().session.as_mut() {
        session.annotation_access = AnnotationAccess::Forbidden;
    }
    crate::app::annotations::update_annotation_controls(&ui.viewer);
    let refusal = ui
        .viewer
        .annotation_editing_refusal()
        .expect("the fixture forbids annotation");

    let outcome = entry(Target::Annotation(Tool::Ink)).run(&ui.window, &ui.viewer);

    assert_eq!(outcome, Err(refusal));
    assert_eq!(ui.viewer.state.borrow().active_tool, None);
}

/// The same central check covers a click tool with no gate of its own
/// before its chooser: signing on a document without one is refused.
#[gtk::test]
fn gtk_ui_running_a_refused_signing_entry_returns_its_refusal() {
    let built = window();
    let ui = &built.0;

    let outcome = entry(Target::Click(ClickTool::SignWithFile)).run(&ui.window, &ui.viewer);

    assert_eq!(outcome, Err(tr("sign-no-document")));
    assert_eq!(
        ui.viewer.view_stack.visible_child_name().as_deref(),
        Some(HOME_PAGE),
        "a refused entry must not even switch screens"
    );
}

// --- T-231: search synonyms -------------------------------------------------

/// Most words a keyword list may hold: enough for the synonyms a user
/// reaches for, short enough that it stays a list and not a paragraph.
const MAX_KEYWORDS: usize = 10;

/// Most entries a common, unspecific query may return. "pdf" and
/// "page"/"pagina" are the words a user types about this app in general;
/// the commands genuinely about them are the PDF and page ones (new blank
/// PDF, add PDFs, organize, extract, split) plus the scattered subsequence
/// hits the matcher reaches without any keyword ("Export images" holds
/// p-a-g-e, "Campo de texto … formulario" holds p-d-f). Measured with the
/// T-231 lists: 4-7 hits per query, the genuine ones first. Eight keeps
/// that one entry of slack and still fails as soon as a keyword list
/// stuffs these words into unrelated commands: out of 35 entries, a query
/// returning more than eight has stopped narrowing anything.
const MAX_COMMON_QUERY_HITS: usize = 8;

#[test]
fn every_keywords_key_resolves_in_english_and_spanish() {
    for entry in catalog() {
        let key = entry.keywords_key();
        for language in [Language::English, Language::Spanish] {
            assert!(
                has_message(language, &key),
                "{} has no {language:?} keywords {key:?}",
                entry.id
            );
        }
    }
}

#[test]
fn keyword_lists_stay_short() {
    for entry in catalog() {
        for language in [Language::English, Language::Spanish] {
            let words = tr_in(language, &entry.keywords_key())
                .split_whitespace()
                .count();
            assert!(
                (1..=MAX_KEYWORDS).contains(&words),
                "{} has {words} {language:?} keywords",
                entry.id
            );
        }
    }
}

#[gtk::test]
fn gtk_ui_spanish_keywords_find_their_commands() {
    let built = window();
    let viewer = &built.0.viewer;
    let _spanish = speak(Language::Spanish);

    let pagina = found_ids(viewer, "pagina");
    for id in ["mode.organize", "pages.extract", "pages.split"] {
        assert!(pagina.contains(&id), "pagina misses {id}: {pagina:?}");
    }
    assert_eq!(pagina.first(), Some(&"mode.organize"));
    assert_eq!(
        found_ids(viewer, "unir pdf").first(),
        Some(&"pages.add-pdfs")
    );
    assert_eq!(
        found_ids(viewer, "reducir").first(),
        Some(&"document.compress")
    );
    let firma = found_ids(viewer, "firma");
    assert!(
        firma
            .first()
            .is_some_and(|id| id.starts_with("sign.") || *id == "mode.sign"),
        "firma: {firma:?}"
    );
    assert!(firma.contains(&"sign.draw"), "firma: {firma:?}");
}

#[gtk::test]
fn gtk_ui_english_keywords_find_their_commands() {
    let built = window();
    let viewer = &built.0.viewer;
    assert_eq!(found_ids(viewer, "merge").first(), Some(&"pages.add-pdfs"));
    assert_eq!(
        found_ids(viewer, "shrink").first(),
        Some(&"document.compress")
    );
}

/// English keywords are searched in every language, like English labels.
#[gtk::test]
fn gtk_ui_english_keywords_match_while_speaking_spanish() {
    let built = window();
    let _spanish = speak(Language::Spanish);
    assert_eq!(
        found_ids(&built.0.viewer, "merge").first(),
        Some(&"pages.add-pdfs")
    );
}

/// The rule is the active language plus English: Spanish keywords are not
/// searched while the window speaks English.
#[gtk::test]
fn gtk_ui_spanish_keywords_are_not_searched_in_english() {
    let built = window();
    assert!(!found_ids(&built.0.viewer, "fusionar").contains(&"pages.add-pdfs"));
}

#[gtk::test]
fn gtk_ui_common_queries_stay_narrow() {
    let built = window();
    let viewer = &built.0.viewer;
    for (language, query) in [
        (Language::English, "pdf"),
        (Language::English, "page"),
        (Language::Spanish, "pdf"),
        (Language::Spanish, "pagina"),
    ] {
        let _language = speak(language);
        let found = found_ids(viewer, query);
        assert!(
            found.len() <= MAX_COMMON_QUERY_HITS,
            "{language:?} {query:?} returns {} entries: {found:?}",
            found.len()
        );
    }
}
