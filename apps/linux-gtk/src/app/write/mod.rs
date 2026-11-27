//! The write chains: every path that turns the open session into bytes.
//!
//! Seven operations live here. Three of them — [`save`], [`sign`] and
//! [`protect`] — write a file the user names, and they are near-identical by
//! design: ask where to write, guard an overwrite, run a writer on a worker
//! thread, then fold the reopened document back into the session. The fourth,
//! [`preview`], reaches the same writer with no destination at all.
//!
//! An ordinary Save is the one of the three that may skip the asking: when
//! the document came from a file it can replace, [`in_place`] hands the chain
//! that file as its destination. Save As, signing and protecting always ask.
//!
//! [`extract`] and [`split`] take one half from each side: they name a
//! destination and guard its overwrite like the first three, and install
//! nothing like the fourth. That combination is what both *are* — new files
//! built from the open document's pages, leaving the session the user is
//! looking at exactly as it was. They differ only in how many files come out,
//! which is why the pruning they both do lives once in
//! `pdf_document::prune` (moved out of this crate once `pdf-ffi`'s extract
//! surface needed the same cut) and why a split asks for a folder where an
//! extraction asks for a name.
//!
//! [`compress`] is the seventh, and the only one that reverses the order:
//! it runs its writer *first* and asks for a destination afterwards, because
//! a compression is the one operation whose worth cannot be known until it has
//! run. See that module's header — the ordering is the feature, not a
//! shortcut.
//!
//! [`snapshot`] is the eighth: the in-memory full save print and export
//! rasterize. It writes nothing and installs nothing — the reopened handle
//! lives only as long as one job — and it exists because the handle the
//! canvas renders from never holds the annotations the overlay draws.
//!
//! ## What is shared, and why
//!
//! The three destination-taking chains used to carry a verbatim copy each of
//! the file chooser, the "Replace existing PDF?" guard and the worker-thread
//! spawn. Three copies of one dialog is three places to fix a dialog bug in,
//! so the shape lives once in [`chooser`] (everything before a byte exists)
//! and [`worker`] (everything after), and each chain supplies the two things
//! that genuinely differ: the words it speaks in ([`WriteOperation`]) and the
//! writer it runs.
//!
//! What stays in a chain's own file is what only that chain does: which
//! `pdf_save`/`pdf_sign` call it makes, what it must ask the user first, and
//! what it carries across the reopen.
//!
//! ## The vocabulary
//!
//! [`WriteOperation`] is deliberately one value rather than five loose
//! `&'static str` parameters, and the three constants below sit together
//! rather than in the files that use them. Every one of those strings is
//! user-visible, and this is the whole vocabulary of the module's UI — one
//! table, so a reader can see at a glance that `save-cancelled` and
//! `protect-cancelled` are the same slot rather than two unrelated messages.

mod chooser;
mod compress;
mod extract;
mod in_place;
mod preview;
mod protect;
mod save;
mod sign;
mod snapshot;
mod split;
mod worker;

use gtk::FileFilter;

use super::i18n::{tr, tr_args};
use super::state::{ImportedSource, SaveBacking};

/// The status for a job whose result was set aside because the document
/// changed while it ran — the one case where nothing was written. A job that
/// wrote its files reports them, and one that failed reports the failure,
/// whatever the document did meanwhile.
pub(crate) fn report_result_unused(viewer: &super::state::Viewer) {
    viewer.status.set_text(&tr("job-document-changed"));
}

/// The overwrite guard on its own, for tests that drive its question.
#[cfg(test)]
pub(crate) fn confirm_destination_for_test(
    window: &gtk::ApplicationWindow,
    viewer: &super::state::Viewer,
    destination: std::path::PathBuf,
    proceed: chooser::Proceed,
) {
    chooser::confirm_destination(window, viewer, SAVE, destination, proceed);
}

pub(crate) use compress::begin_compress;
pub(crate) use extract::begin_extract;
pub(crate) use preview::{refresh_preview, restore_view_state, take_view_state, ViewState};
pub(crate) use protect::{begin_protect, ProtectRequest};
pub(crate) use save::{save, save_as, save_then};
pub(crate) use sign::{begin_sign, SignRequest};
pub(crate) use snapshot::{snapshot_for_output, OutputSource};
pub(crate) use split::begin_split;

/// The imported documents a save has to graft from, in the shape
/// `pdf_save::ImportedSources` borrows them in.
fn imported_sources(
    sources: &[ImportedSource],
) -> Vec<(pdf_document::ImportedDocumentId, &pdf_manip::LopdfDocument)> {
    sources
        .iter()
        .map(|source| (source.id, &source.document))
        .collect()
}

/// Whether a save breaks a signature, given the core's answer about the
/// save itself.
///
/// `base_breaks_signatures` is the one case `pdf-save` cannot see: a document
/// restored after a crash from bytes that had already broken the signature.
/// And the answer fails closed: a question the core could not answer is a
/// signature that may break, so the user is asked rather than told nothing.
fn breaks_signature<E>(
    base_breaks_signatures: bool,
    core_answer: impl FnOnce() -> Result<bool, E>,
) -> bool {
    base_breaks_signatures || core_answer().unwrap_or(true)
}

/// [`breaks_signature`] for the ordinary Save — what `save` asks before
/// writing and what every autosave snapshot records in its sidecar.
pub(crate) fn default_save_breaks_signature(
    document: &pdf_document::Document,
    backing: &SaveBacking,
    sources: &[ImportedSource],
    base_breaks_signatures: bool,
) -> bool {
    breaks_signature(base_breaks_signatures, || {
        let source_refs = imported_sources(sources);
        pdf_save::will_invalidate_signatures(pdf_save::SaveInput {
            document,
            base: &backing.base,
            original_bytes: Some(&backing.original_bytes),
            intent: pdf_save::SaveIntent::Default,
            signatures: pdf_save::SignatureAcknowledgement::Unacknowledged,
            imported_sources: pdf_save::ImportedSources::new(&source_refs),
        })
    })
}

/// The PDF filter every file dialog in this shell offers — the open chooser
/// in [`super::document`] included, which is why this is `pub(crate)` rather
/// than private to the write chains.
pub(crate) fn pdf_filter() -> FileFilter {
    let filter = FileFilter::new();
    filter.set_name(Some(&tr("write-pdf-filter")));
    filter.add_mime_type("application/pdf");
    filter.add_pattern("*.pdf");
    filter.add_pattern("*.PDF");
    filter
}

/// What one write operation calls itself, everywhere it has to speak.
///
/// Five strings, one table. The alternative — each chain carrying its own
/// literals through its own copy of the chooser and the replace guard — is
/// what had three separately maintained dialogs saying the same sentence in
/// the first place.
///
/// Every field is a Fluent message id, not text — the English lives in
/// `locales/en.ftl`. Read the strings through the methods below, which render
/// them in the user's language, never the fields.
#[derive(Clone, Copy)]
struct WriteOperation {
    /// The destination chooser's title.
    title: &'static str,
    /// What the status bar says when the user backs out — at the chooser, at
    /// the replace guard, or at the signature warning.
    cancelled: &'static str,
    /// Shown from the moment the worker starts.
    busy: &'static str,
    /// Shown once the written document has been reopened and installed.
    done: &'static str,
    /// The failure sentence, which takes the worker's own error text verbatim
    /// as `{ $error }`.
    failed: &'static str,
}

impl WriteOperation {
    fn title(self) -> String {
        tr(self.title)
    }

    fn cancelled(self) -> String {
        tr(self.cancelled)
    }

    fn busy(self) -> String {
        tr(self.busy)
    }

    fn done(self) -> String {
        tr(self.done)
    }

    /// The failure sentence for the worker's own `error` text, which stays
    /// verbatim.
    fn failed(self, error: &str) -> String {
        tr_args(self.failed, &[("error", error.into())])
    }
}

const SAVE: WriteOperation = WriteOperation {
    title: "save-chooser-title",
    cancelled: "save-cancelled",
    busy: "save-busy",
    done: "save-done",
    failed: "save-failed",
};

/// `cancelled` is the same sentence the identity picker shows when the user
/// backs out there, so it shares that message rather than repeating it.
const SIGN: WriteOperation = WriteOperation {
    title: "sign-chooser-title",
    cancelled: "sign-cancelled",
    busy: "sign-busy",
    done: "sign-done",
    failed: "sign-failed",
};

/// `cancelled` is also what `app::protect`'s own Cancel button says.
const PROTECT: WriteOperation = WriteOperation {
    title: "protect-chooser-title",
    cancelled: "protect-cancelled",
    busy: "protect-busy",
    done: "protect-done",
    failed: "protect-failed",
};

/// The last two operations' vocabulary — and the two entries in this table
/// whose `done` is not what the user sees.
///
/// [`extract`] and [`split`] report through their own
/// `options::*_summary` instead, because each completion sentence names a
/// count and a path and so cannot be a constant. The field is filled rather
/// than made optional to keep one shape for all six; `done` stays the slot it
/// is for the three chains that reach `worker::spawn_write`.
const EXTRACT: WriteOperation = WriteOperation {
    title: "extract-chooser-title",
    cancelled: "extract-cancelled",
    busy: "extract-busy",
    done: "extract-done",
    failed: "extract-failed",
};

/// `title` here names a **folder** chooser rather than a save dialog — see
/// [`split`] for why a chain that writes one file per cut has no single
/// destination to ask for.
const SPLIT: WriteOperation = WriteOperation {
    title: "split-chooser-title",
    cancelled: "split-cancelled",
    busy: "split-busy",
    done: "split-done",
    failed: "split-failed",
};

/// The third entry whose `done` the user never sees, and for a sharper reason
/// than [`EXTRACT`]'s: by the time a compression has a destination it has
/// already been reported on, so its ending sentence names a path *and* two
/// sizes (`compress::options::written_summary`). `busy` covers the
/// compression itself; the shorter write that follows has its own line in
/// `compress::options`, because "Compressing PDF..." shown twice reads as the
/// work starting over.
const COMPRESS: WriteOperation = WriteOperation {
    title: "compress-chooser-title",
    cancelled: "compress-cancelled",
    busy: "compress-busy",
    done: "compress-done",
    failed: "compress-failed",
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::i18n::{speak, Language};

    /// A signature question the core could not answer is asked of the user.
    #[test]
    fn an_unanswered_signature_question_fails_closed() {
        assert!(breaks_signature(false, || Err::<bool, ()>(())));
        assert!(!breaks_signature(false, || Ok::<bool, ()>(false)));
        assert!(breaks_signature(true, || Ok::<bool, ()>(false)));
    }

    /// Every operation, in table order, with the English it spoke before its
    /// strings moved into the catalogs: `[title, cancelled, busy, done,
    /// failed(error)]`.
    fn english() -> [(WriteOperation, [&'static str; 5]); 6] {
        [
            (
                SAVE,
                [
                    "Save PDF",
                    "Save cancelled.",
                    "Saving PDF...",
                    "PDF saved and reopened.",
                    "Could not save PDF: disk {full}",
                ],
            ),
            (
                SIGN,
                [
                    "Save signed PDF",
                    "Signing cancelled.",
                    "Signing PDF...",
                    "PDF signed and reopened.",
                    "Could not sign PDF: disk {full}",
                ],
            ),
            (
                PROTECT,
                [
                    "Save protected PDF",
                    "Protection cancelled.",
                    "Protecting PDF...",
                    "PDF protected, saved and reopened.",
                    "Could not protect PDF: disk {full}",
                ],
            ),
            (
                EXTRACT,
                [
                    "Save extracted PDF",
                    "Extraction cancelled.",
                    "Extracting pages...",
                    "Pages extracted.",
                    "Could not extract pages: disk {full}",
                ],
            ),
            (
                SPLIT,
                [
                    "Split PDF into folder",
                    "Split cancelled.",
                    "Splitting PDF...",
                    "PDF split.",
                    "Could not split PDF: disk {full}",
                ],
            ),
            (
                COMPRESS,
                [
                    "Save compressed PDF",
                    "Compression cancelled.",
                    "Compressing PDF...",
                    "PDF compressed.",
                    "Could not compress PDF: disk {full}",
                ],
            ),
        ]
    }

    fn spoken(operation: WriteOperation) -> [String; 5] {
        [
            operation.title(),
            operation.cancelled(),
            operation.busy(),
            operation.done(),
            operation.failed("disk {full}"),
        ]
    }

    #[test]
    fn every_operation_speaks_its_original_english() {
        for (operation, expected) in english() {
            assert_eq!(spoken(operation), expected.map(str::to_owned));
        }
    }

    #[test]
    fn save_sign_and_protect_speak_spanish() {
        let _spanish = speak(Language::Spanish);
        assert_eq!(
            spoken(SAVE),
            [
                "Guardar PDF",
                "Guardado cancelado.",
                "Guardando el PDF...",
                "PDF guardado y vuelto a abrir.",
                "No se pudo guardar el PDF: disk {full}",
            ]
            .map(str::to_owned)
        );
        assert_eq!(
            spoken(SIGN),
            [
                "Guardar el PDF firmado",
                "Firma cancelada.",
                "Firmando el PDF...",
                "PDF firmado y vuelto a abrir.",
                "No se pudo firmar el PDF: disk {full}",
            ]
            .map(str::to_owned)
        );
        assert_eq!(
            spoken(PROTECT),
            [
                "Guardar el PDF protegido",
                "Protección cancelada.",
                "Protegiendo el PDF...",
                "PDF protegido, guardado y vuelto a abrir.",
                "No se pudo proteger el PDF: disk {full}",
            ]
            .map(str::to_owned)
        );
    }

    /// The filter name and the Save chain's own refusals, which speak before
    /// any [`WriteOperation`] does.
    fn save_text() -> [String; 3] {
        ["write-pdf-filter", "save-no-document", "save-not-editable"].map(tr)
    }

    #[test]
    fn the_save_refusals_and_filter_keep_their_english() {
        assert_eq!(
            save_text(),
            [
                "PDF files",
                "Open a PDF before saving.",
                "This document cannot be saved as an editable PDF.",
            ]
        );
    }

    #[test]
    fn the_save_refusals_and_filter_speak_spanish() {
        let _spanish = speak(Language::Spanish);
        assert_eq!(
            save_text(),
            [
                "Archivos PDF",
                "Abre un PDF antes de guardarlo.",
                "Este documento no se puede guardar como un PDF editable.",
            ]
        );
    }
}
