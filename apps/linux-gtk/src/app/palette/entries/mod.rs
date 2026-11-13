//! The command palette's catalog (B27 T-229): which commands exist, what
//! they are called, which shortcut they show, whether they can run right
//! now and what running one does.
//!
//! An entry never carries behaviour of its own (decision 2). Its [`Target`]
//! names an existing door — a mode button's [`Mode::open`], a `win.*`
//! action, an annotation or form-field toggle, or the named function a
//! toolbar button calls — and [`invoke`] only walks through it, after at
//! most a screen switch: to the editor's tools page for annotation, form and
//! signing entries, to Organize for Add PDFs, to the editor for Find. Whether
//! it can is answered by the same `*_refusal` functions that gate the buttons
//! (decision 3), so a refused entry states the button's own reason; undo and
//! redo follow their action's `is_enabled()`.
//!
//! Page rotation is in the catalog as two whole-document entries, Rotate
//! all pages right and left (added after T-234). They walk through
//! `organize::rotate_all_pages`, which is the Documents view's block turn
//! (`rotate_block`) over every page: same permission gate, same single
//! `Command::RotatePages` undo step, same thumbnail invalidation.

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
    /// Organize's page tools: add PDFs, extract, split, rotate all pages.
    Pages,
    /// Signing.
    Sign,
}

/// The click-only tools: commands whose only door before the palette was a
/// button's `connect_clicked`. Each names the function that button calls —
/// except Compress, a Home tool, which goes through `open_tool` like its
/// Home card, and the two whole-document turns, which call
/// `organize::rotate_all_pages`: the Documents view's block-turn function
/// over every page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClickTool {
    ExportImages,
    Watermark,
    Compress,
    AddPdfs,
    Extract,
    Split,
    RotateAllRight,
    RotateAllLeft,
    SignWithFile,
    SignWithToken,
    SignWithSystemCertificate,
    DrawSignature,
}

impl ClickTool {
    const ALL: [ClickTool; 12] = [
        ClickTool::ExportImages,
        ClickTool::Watermark,
        ClickTool::Compress,
        ClickTool::AddPdfs,
        ClickTool::Extract,
        ClickTool::Split,
        ClickTool::RotateAllRight,
        ClickTool::RotateAllLeft,
        ClickTool::SignWithFile,
        ClickTool::SignWithToken,
        ClickTool::SignWithSystemCertificate,
        ClickTool::DrawSignature,
    ];
}

/// The `win.*` actions the catalog lists. A closed set, so the shortcut
/// each one activates is an exhaustive `match` the compiler checks, not a
/// lookup that can miss at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WindowAction {
    Open,
    New,
    Save,
    Print,
    Undo,
    Redo,
    Find,
}

impl WindowAction {
    /// The action and accelerator from the shared table.
    pub(crate) fn shortcut(self) -> Shortcut {
        match self {
            WindowAction::Open => shortcuts::OPEN,
            WindowAction::New => shortcuts::NEW,
            WindowAction::Save => shortcuts::SAVE,
            WindowAction::Print => shortcuts::PRINT,
            WindowAction::Undo => shortcuts::UNDO,
            WindowAction::Redo => shortcuts::REDO,
            WindowAction::Find => shortcuts::FIND,
        }
    }
}

/// The existing door an entry walks through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Target {
    Mode(Mode),
    Action(WindowAction),
    Annotation(Tool),
    FormField(FieldKind),
    Click(ClickTool),
}

/// One command in the palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PaletteEntry {
    /// Stable id, unique in the catalog; the recent-commands list keys on it.
    pub(crate) id: &'static str,
    /// Message id of the label, reused from the button the entry mirrors.
    pub(crate) label_key: &'static str,
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

    /// Runs the entry through its existing door, unless [`availability`]
    /// refuses it now — then nothing runs and the refusal comes back.
    ///
    /// Checked here, once for every target, because the doors do not all
    /// refuse on their own: `set_active(true)` on an insensitive annotation
    /// or form-field toggle still runs its handler and arms the tool, and
    /// the signing entries open their chooser with no gate in front.
    ///
    /// [`availability`]: PaletteEntry::availability
    pub(crate) fn run(&self, window: &ApplicationWindow, viewer: &Viewer) -> Result<(), String> {
        if let Availability::Refused(reason) = self.availability(viewer) {
            return Err(reason);
        }
        invoke::run(self.target, window, viewer);
        Ok(())
    }

    /// Message id of the search synonyms, `palette-<id>-keywords` with the
    /// id's dots as dashes (Fluent ids allow no dots). Derived, not stored,
    /// so an entry cannot point at another entry's list.
    pub(crate) fn keywords_key(&self) -> String {
        format!("palette-{}-keywords", self.id.replace('.', "-"))
    }

    /// The texts a query is matched against: the label followed by its
    /// synonyms, once in the active language and once in English. Label and
    /// synonyms share one text so a query may mix them ("unir pdf" finds
    /// "Añadir PDF"); English rides along in every language like the English
    /// label, but another language's synonyms are never searched.
    fn haystacks(&self) -> Vec<String> {
        let keywords = self.keywords_key();
        let english = |key: &str| tr_in(Language::English, key);
        vec![
            format!("{} {}", tr(self.label_key), tr(&keywords)),
            format!("{} {}", english(self.label_key), english(&keywords)),
        ]
    }
}

/// Every command, in catalog order: modes, file, edit, annotate, forms,
/// document, pages, sign.
pub(crate) fn catalog() -> Vec<PaletteEntry> {
    let modes = Mode::ALL.into_iter().map(|mode| PaletteEntry {
        id: mode_id(mode),
        label_key: mode.label_key(),
        shortcut: None,
        category: Category::Mode,
        target: Target::Mode(mode),
    });
    let actions = [
        (
            "file.open",
            WindowAction::Open,
            "home-quick-open",
            Category::File,
        ),
        (
            "file.new",
            WindowAction::New,
            "home-quick-new-blank",
            Category::File,
        ),
        (
            "file.save",
            WindowAction::Save,
            "editor-save",
            Category::File,
        ),
        (
            "file.print",
            WindowAction::Print,
            "editor-print",
            Category::File,
        ),
        (
            "edit.undo",
            WindowAction::Undo,
            "editor-undo",
            Category::Edit,
        ),
        (
            "edit.redo",
            WindowAction::Redo,
            "editor-redo",
            Category::Edit,
        ),
        (
            "edit.find",
            WindowAction::Find,
            "editor-find",
            Category::Edit,
        ),
    ]
    .into_iter()
    .map(|(id, action, label_key, category)| PaletteEntry {
        id,
        label_key,
        shortcut: Some(action.shortcut().accel),
        category,
        target: Target::Action(action),
    });
    let tools = Tool::ALL.into_iter().map(|tool| PaletteEntry {
        id: tool_id(tool),
        label_key: tool.label_key(),
        shortcut: None,
        category: Category::Annotate,
        target: Target::Annotation(tool),
    });
    let fields = FieldKind::ALL.into_iter().map(|kind| PaletteEntry {
        id: field_id(kind),
        label_key: kind.label_key(),
        shortcut: None,
        category: Category::Forms,
        target: Target::FormField(kind),
    });
    let clicks = ClickTool::ALL.into_iter().map(|tool| {
        let (id, label_key, category) = click_meta(tool);
        PaletteEntry {
            id,
            label_key,
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
/// (decision 8) is the overlay's job (`palette::recent`).
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
        ClickTool::RotateAllRight => (
            "pages.rotate-all-right",
            "palette-rotate-all-right",
            Category::Pages,
        ),
        ClickTool::RotateAllLeft => (
            "pages.rotate-all-left",
            "palette-rotate-all-left",
            Category::Pages,
        ),
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
