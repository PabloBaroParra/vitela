//! The command palette's catalog (B27 T-229): which commands exist, what
//! they are called, which shortcut they show, whether they can run right
//! now and what running one does.
//!
//! An entry never carries behaviour of its own (decision 2). Its [`Target`]
//! names an existing door — a mode button's [`Mode::open`], a `win.*`
//! action, an annotation or form-field toggle, or the named function a
//! toolbar button calls — and [`invoke`] only walks through it. Whether it
//! can is answered by the same `*_refusal` functions that gate the buttons
//! (decision 3), so a refused entry states the button's own reason.
//!
//! Page rotation is not in the catalog: its buttons turn the page or block
//! of one Organize card, and there is no document-level rotate for the
//! palette to call.

// Caller lands in T-230 (palette overlay).
#![cfg_attr(not(test), allow(dead_code))]

mod invoke;

use gtk::ApplicationWindow;

use super::matcher;
use crate::app::i18n::{tr, tr_in, Language};
use crate::app::mode_bar::Mode;
use crate::app::shortcuts::{self, Shortcut};
use crate::app::state::{FieldKind, Tool, Viewer};

/// Whether an entry can run right now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Availability {
    Available,
    /// Shown disabled, with this sentence as the reason.
    Refused(String),
}

/// The group an entry is listed under when the search is empty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Category {
    /// The mode bar's sections.
    Mode,
    /// Open, new, save, print.
    File,
    /// Undo, redo, find.
    Edit,
    /// Annotation creation tools.
    Annotate,
    /// Form-field placement kinds.
    Forms,
    /// Whole-document tools: export images, watermark, compress.
    Document,
    /// Organize's page tools: add PDFs, extract, split.
    Pages,
    /// Signing.
    Sign,
}

/// The click-only tools: commands whose only door before the palette was a
/// button's `connect_clicked`. Each names the function that button calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickTool {
    ExportImages,
    Watermark,
    Compress,
    AddPdfs,
    Extract,
    Split,
    SignWithFile,
    SignWithToken,
    SignWithSystemCertificate,
    DrawSignature,
}

impl ClickTool {
    const ALL: [ClickTool; 10] = [
        ClickTool::ExportImages,
        ClickTool::Watermark,
        ClickTool::Compress,
        ClickTool::AddPdfs,
        ClickTool::Extract,
        ClickTool::Split,
        ClickTool::SignWithFile,
        ClickTool::SignWithToken,
        ClickTool::SignWithSystemCertificate,
        ClickTool::DrawSignature,
    ];
}

/// The existing door an entry walks through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    Mode(Mode),
    Action(Shortcut),
    Annotation(Tool),
    FormField(FieldKind),
    Click(ClickTool),
}

/// One command in the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaletteEntry {
    /// Stable id, unique in the catalog (for T-230's recent-commands list).
    pub(crate) id: &'static str,
    /// Message id of the label, reused from the button the entry mirrors.
    pub(crate) label_key: &'static str,
    /// Message id of the search synonyms; `None` until T-231 writes them.
    pub(crate) keywords_key: Option<&'static str>,
    /// Accelerator in GTK syntax, read from [`shortcuts`].
    pub(crate) shortcut: Option<&'static str>,
    pub(crate) category: Category,
    pub(crate) target: Target,
}

impl PaletteEntry {
    /// Whether this entry can run right now, and why not.
    pub(crate) fn availability(&self, viewer: &Viewer) -> Availability {
        invoke::availability(self.target, viewer)
    }

    /// Runs the entry through its existing door. A refused entry is never
    /// meant to be run; the door it walks through refuses it again anyway.
    pub(crate) fn run(&self, window: &ApplicationWindow, viewer: &Viewer) {
        invoke::run(self.target, window, viewer);
    }

    /// The texts a query is matched against: the label in the active
    /// language, the label in English, and the synonyms when there are any.
    fn haystacks(&self) -> Vec<String> {
        let mut texts = vec![tr(self.label_key), tr_in(Language::English, self.label_key)];
        if let Some(key) = self.keywords_key {
            texts.push(tr(key));
        }
        texts
    }
}

/// Every command, in catalog order: modes, file, edit, annotate, forms,
/// document, pages, sign.
pub(crate) fn catalog() -> Vec<PaletteEntry> {
    let modes = Mode::ALL.into_iter().map(|mode| PaletteEntry {
        id: mode_id(mode),
        label_key: mode.label_key(),
        keywords_key: None,
        shortcut: None,
        category: Category::Mode,
        target: Target::Mode(mode),
    });
    let actions = [
        (
            "file.open",
            shortcuts::OPEN,
            "home-quick-open",
            Category::File,
        ),
        (
            "file.new",
            shortcuts::NEW,
            "home-quick-new-blank",
            Category::File,
        ),
        ("file.save", shortcuts::SAVE, "editor-save", Category::File),
        (
            "file.print",
            shortcuts::PRINT,
            "editor-print",
            Category::File,
        ),
        ("edit.undo", shortcuts::UNDO, "editor-undo", Category::Edit),
        ("edit.redo", shortcuts::REDO, "editor-redo", Category::Edit),
        ("edit.find", shortcuts::FIND, "editor-find", Category::Edit),
    ]
    .into_iter()
    .map(|(id, shortcut, label_key, category)| PaletteEntry {
        id,
        label_key,
        keywords_key: None,
        shortcut: Some(shortcut.accel),
        category,
        target: Target::Action(shortcut),
    });
    let tools = Tool::ALL.into_iter().map(|tool| PaletteEntry {
        id: tool_id(tool),
        label_key: tool.label_key(),
        keywords_key: None,
        shortcut: None,
        category: Category::Annotate,
        target: Target::Annotation(tool),
    });
    let fields = FieldKind::ALL.into_iter().map(|kind| PaletteEntry {
        id: field_id(kind),
        label_key: kind.label_key(),
        keywords_key: None,
        shortcut: None,
        category: Category::Forms,
        target: Target::FormField(kind),
    });
    let clicks = ClickTool::ALL.into_iter().map(|tool| {
        let (id, label_key, category) = click_meta(tool);
        PaletteEntry {
            id,
            label_key,
            keywords_key: None,
            shortcut: None,
            category,
            target: Target::Click(tool),
        }
    });
    modes
        .chain(actions)
        .chain(tools)
        .chain(fields)
        .chain(clicks)
        .collect()
}

/// The catalog ranked against `query`, each with its current availability
/// and its score. Refused entries stay in the list (decision 3). An empty
/// query keeps catalog order; putting the session's recent commands first
/// (decision 8) is the overlay's job.
pub(crate) fn search(viewer: &Viewer, query: &str) -> Vec<(PaletteEntry, Availability, u32)> {
    let entries = catalog();
    matcher::rank(query, &entries, PaletteEntry::haystacks)
        .into_iter()
        .map(|(index, score)| {
            let entry = entries[index];
            (entry, entry.availability(viewer), score)
        })
        .collect()
}

fn mode_id(mode: Mode) -> &'static str {
    match mode {
        Mode::Home => "mode.home",
        Mode::Edit => "mode.edit",
        Mode::Annotate => "mode.annotate",
        Mode::Organize => "mode.organize",
        Mode::Sign => "mode.sign",
        Mode::Protect => "mode.protect",
    }
}

fn tool_id(tool: Tool) -> &'static str {
    match tool {
        Tool::Highlight => "annotate.highlight",
        Tool::Underline => "annotate.underline",
        Tool::Strikeout => "annotate.strikeout",
        Tool::Ink => "annotate.ink",
        Tool::TextNote => "annotate.note",
        Tool::FreeText => "annotate.text-box",
        Tool::Shape => "annotate.shape",
        Tool::Stamp => "annotate.stamp",
    }
}

fn field_id(kind: FieldKind) -> &'static str {
    match kind {
        FieldKind::Text => "forms.text",
        FieldKind::Checkbox => "forms.checkbox",
        FieldKind::RadioGroup => "forms.radio-group",
        FieldKind::Dropdown => "forms.dropdown",
    }
}

/// Id, label (the button's own message id) and category of a click tool.
fn click_meta(tool: ClickTool) -> (&'static str, &'static str, Category) {
    match tool {
        ClickTool::ExportImages => (
            "document.export-images",
            "editor-export",
            Category::Document,
        ),
        ClickTool::Watermark => ("document.watermark", "edit-watermark", Category::Document),
        ClickTool::Compress => (
            "document.compress",
            "home-tool-compress",
            Category::Document,
        ),
        ClickTool::AddPdfs => ("pages.add-pdfs", "organize-add-pdfs", Category::Pages),
        ClickTool::Extract => ("pages.extract", "organize-extract", Category::Pages),
        ClickTool::Split => ("pages.split", "organize-split", Category::Pages),
        ClickTool::SignWithFile => ("sign.certificate-file", "sign-choose-pfx", Category::Sign),
        ClickTool::SignWithToken => ("sign.token", "sign-use-token", Category::Sign),
        ClickTool::SignWithSystemCertificate => {
            ("sign.system-certificate", "sign-use-nss", Category::Sign)
        }
        ClickTool::DrawSignature => ("sign.draw", "signature-draw", Category::Sign),
    }
}

#[cfg(test)]
mod tests;
