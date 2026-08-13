//! Error type for content-stream parsing, encoding, editing and insertion.

use pdf_document::{ContentItemId, PageId};
use std::fmt;

/// Errors surfaced by this crate.
///
/// `#[non_exhaustive]`, matching `AnnotateError` and `pdf-document`'s
/// `Command`: lifting Type0/CID text editing is an explicit post-v1
/// candidate and will need its own failure modes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EditError {
    /// The document has no page with this id.
    PageNotFound(PageId),
    /// The page has no item with this id — most often because the ids came
    /// from a *different* revision of the document. Ids are assigned by the
    /// parser in stream order and are only meaningful against the exact
    /// bytes they were parsed from (see [`crate::parse`]).
    ItemNotFound(ContentItemId),
    /// Several items on the page are indistinguishable from the one the
    /// command targets — same text, font and box, or same XObject painted at
    /// the same place — and the id no longer picks one out. Editing an
    /// arbitrary one of them would be a coin flip on the user's document, so
    /// the edit is refused instead.
    AmbiguousItem { id: ContentItemId, matches: usize },
    /// The content stream could not be tokenized. Carries what was expected
    /// and the byte offset it gave up at.
    MalformedContent { reason: String, offset: usize },
    /// The replacement text contains a character the run's font cannot
    /// represent, so the edit is refused **before** the stream is touched
    /// (batch decision 3). Reported per character rather than as a blanket
    /// "not editable", because that is what a shell needs to tell the user.
    EncodingGap {
        character: char,
        resource_font_name: String,
    },
    /// The run's font is a composite (Type0/CID) font, which v1 does not
    /// edit at all: extending a subsetted CID font's glyph coverage means
    /// re-subsetting it, which is a separate category of work.
    CompositeFontNotEditable { resource_font_name: String },
    /// The font resource named by a run is missing from the page's
    /// `/Resources /Font`, or is not a font dictionary — a malformed file
    /// rather than an unsupported one.
    FontResourceMissing { resource_font_name: String },
    /// Image bytes could not be decoded as a supported format (PNG/JPEG).
    InvalidImage(String),
    /// A new resource was asked to be registered under a name the page
    /// already uses for a different object. Overwriting it would silently
    /// repaint every other operator on the page that names it.
    ResourceNameInUse { category: String, name: String },
    /// A content stream is `FlateDecode`d but does not inflate to a
    /// *complete* stream — truncated, corrupt, or not deflate data at all.
    ///
    /// The prefix that did come out is deliberately discarded rather than
    /// used: it looks like content, and writing it back after an edit would
    /// replace the page with however much of it survived.
    UndecodableContentStream { object_id: (u32, u16) },
    /// A content stream is encoded in a way this crate cannot prove it round
    /// trips: a filter other than `FlateDecode`, a chain of them, an
    /// unresolvable indirect `/Filter`, or any `/DecodeParms`.
    ///
    /// Distinct from [`Self::UndecodableContentStream`] because it says
    /// something different to whoever reads it — the file is fine, this
    /// version simply does not edit that encoding — and because the fix is
    /// different too.
    UnsupportedContentStreamFilter {
        object_id: (u32, u16),
        detail: String,
    },
    /// A structural problem reported by lopdf while reading the document.
    Lopdf(String),
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EditError::PageNotFound(page) => write!(f, "no page with id {}", page.0),
            EditError::ItemNotFound(id) => write!(
                f,
                "no content item with id {} on this page (ids are only valid \
                 against the document revision they were parsed from)",
                id.0
            ),
            EditError::AmbiguousItem { id, matches } => write!(
                f,
                "{matches} items on this page are indistinguishable from the one \
                 with id {} — the edit cannot be aimed at one of them",
                id.0
            ),
            EditError::MalformedContent { reason, offset } => {
                write!(f, "malformed content stream at byte {offset}: {reason}")
            }
            EditError::EncodingGap {
                character,
                resource_font_name,
            } => write!(
                f,
                "font {resource_font_name} cannot represent {character:?}"
            ),
            EditError::CompositeFontNotEditable { resource_font_name } => write!(
                f,
                "font {resource_font_name} is a composite (Type0/CID) font, \
                 which cannot be edited in this version"
            ),
            EditError::FontResourceMissing { resource_font_name } => {
                write!(f, "font resource {resource_font_name} is missing")
            }
            EditError::InvalidImage(msg) => write!(f, "invalid image bytes: {msg}"),
            EditError::ResourceNameInUse { category, name } => write!(
                f,
                "the page already has a /{category} resource named {name}, and \
                 overwriting it would change every operator that paints it"
            ),
            EditError::UndecodableContentStream { object_id } => write!(
                f,
                "content stream {} {} does not decompress to a complete stream",
                object_id.0, object_id.1
            ),
            EditError::UnsupportedContentStreamFilter { object_id, detail } => write!(
                f,
                "content stream {} {} is encoded in a way this version does not edit: {detail}",
                object_id.0, object_id.1
            ),
            EditError::Lopdf(msg) => write!(f, "pdf structure error: {msg}"),
        }
    }
}

impl std::error::Error for EditError {}

impl From<lopdf::Error> for EditError {
    fn from(error: lopdf::Error) -> Self {
        EditError::Lopdf(error.to_string())
    }
}
