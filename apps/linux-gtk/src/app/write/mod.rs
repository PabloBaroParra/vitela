//! The write chains: every path that turns the open session into bytes.
//!
//! Seven operations live here. Three of them — [`save`], [`sign`] and
//! [`protect`] — write a file the user names, and they are near-identical by
//! design: ask where to write, guard an overwrite, run a writer on a worker
//! thread, then fold the reopened document back into the session. The fourth,
//! [`preview`], reaches the same writer with no destination at all.
//!
//! [`extract`] and [`split`] take one half from each side: they name a
//! destination and guard its overwrite like the first three, and install
//! nothing like the fourth. That combination is what both *are* — new files
//! built from the open document's pages, leaving the session the user is
//! looking at exactly as it was. They differ only in how many files come out,
//! which is why the pruning they both do lives once in [`prune`] and why a
//! split asks for a folder where an extraction asks for a name.
//!
//! [`compress`] is the seventh, and the only one that reverses the order:
//! it runs its writer *first* and asks for a destination afterwards, because
//! a compression is the one operation whose worth cannot be known until it has
//! run. See that module's header — the ordering is the feature, not a
//! shortcut.
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
//! table, so a reader can see at a glance that "Save cancelled." and
//! "Protection cancelled." are the same slot rather than two unrelated
//! literals.

mod chooser;
mod compress;
mod extract;
mod preview;
mod protect;
mod prune;
mod save;
mod sign;
mod split;
mod worker;

use gtk::FileFilter;

use super::state::ImportedSource;

pub(crate) use compress::begin_compress;
pub(crate) use extract::begin_extract;
pub(crate) use preview::refresh_preview;
pub(crate) use protect::{begin_protect, ProtectRequest};
pub(crate) use save::{show_save_chooser, show_save_chooser_then};
pub(crate) use sign::{begin_sign, SignRequest};
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

/// The PDF filter every file dialog in this shell offers — the open chooser
/// in [`super::document`] included, which is why this is `pub(crate)` rather
/// than private to the write chains.
pub(crate) fn pdf_filter() -> FileFilter {
    let filter = FileFilter::new();
    filter.set_name(Some("PDF files"));
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
    /// Prefixed to the worker's own error text, as `"{failed}: {error}"`.
    failed: &'static str,
}

const SAVE: WriteOperation = WriteOperation {
    title: "Save PDF",
    cancelled: "Save cancelled.",
    busy: "Saving PDF...",
    done: "PDF saved and reopened.",
    failed: "Could not save PDF",
};

const SIGN: WriteOperation = WriteOperation {
    title: "Save signed PDF",
    cancelled: "Signing cancelled.",
    busy: "Signing PDF...",
    done: "PDF signed and reopened.",
    failed: "Could not sign PDF",
};

const PROTECT: WriteOperation = WriteOperation {
    title: "Save protected PDF",
    cancelled: "Protection cancelled.",
    busy: "Protecting PDF...",
    done: "PDF protected, saved and reopened.",
    failed: "Could not protect PDF",
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
    title: "Save extracted PDF",
    cancelled: "Extraction cancelled.",
    busy: "Extracting pages...",
    done: "Pages extracted.",
    failed: "Could not extract pages",
};

/// `title` here names a **folder** chooser rather than a save dialog — see
/// [`split`] for why a chain that writes one file per cut has no single
/// destination to ask for.
const SPLIT: WriteOperation = WriteOperation {
    title: "Split PDF into folder",
    cancelled: "Split cancelled.",
    busy: "Splitting PDF...",
    done: "PDF split.",
    failed: "Could not split PDF",
};

/// The third entry whose `done` the user never sees, and for a sharper reason
/// than [`EXTRACT`]'s: by the time a compression has a destination it has
/// already been reported on, so its ending sentence names a path *and* two
/// sizes (`compress::options::written_summary`). `busy` covers the
/// compression itself; the shorter write that follows has its own line in
/// `compress::options`, because "Compressing PDF..." shown twice reads as the
/// work starting over.
const COMPRESS: WriteOperation = WriteOperation {
    title: "Save compressed PDF",
    cancelled: "Compression cancelled.",
    busy: "Compressing PDF...",
    done: "PDF compressed.",
    failed: "Could not compress PDF",
};
