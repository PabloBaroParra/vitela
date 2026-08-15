//! `pdf-edit`: editing the content *inside* a page — the text runs and
//! images painted by the page's content stream — as opposed to annotations
//! and form fields, which are addressable objects hanging off the page
//! dictionary (Batch 21, T-151..T-155).
//!
//! - [`parse`] (T-152): tokenizes a page's content stream and interprets it
//!   far enough to locate every text run and image and compute its bounding
//!   box in page space. Produces `pdf_document::PageContent`.
//! - [`encoding`] (T-153): resolves a font resource to its simple encoding
//!   and metrics, and encodes replacement text against it — reporting an
//!   `EncodingGap` rather than writing a glyph the font cannot show.
//! - [`edit`] (T-154): surgical rewrites of an existing stream — replace a
//!   run's text, move/resize/replace/delete an image — leaving every byte
//!   it did not target untouched.
//! - [`insert`] (T-155): appends genuinely new text or images as page
//!   content, registering the resources they need.
//! - [`error`]: `EditError`, the shared error type across this crate.
//!
//! This crate is deliberately isolated from the rest of the core (the same
//! posture as `pdf-sign`): a future viewer-only build must not have to drag
//! a content-stream interpreter in with it. It depends on `pdf-document`
//! for the data model and `lopdf` for file structure, and on nothing else.
//!
//! **Page content is never cached in `Document`** (batch decision 2). Every
//! entry point here takes the `lopdf::Document` and reads what it needs, on
//! demand, the moment a shell asks — which is the first time the user opens
//! content-edit mode for a page, not document open.
//!
//! ## Naming the page
//!
//! [`read_page_content`] takes a `PageId` and resolves it positionally,
//! which is what a shell reading an untouched document wants. Every
//! *editing* entry point takes the page's `lopdf::ObjectId` instead, because
//! by the time edits are replayed the save may already have deleted,
//! reordered or inserted pages — and a `PageId` is a position, so resolving
//! one then lands the edit on whichever page moved into that slot.
//! [`page_object_id`] does the positional resolution for callers that know
//! their document has not moved underneath them; `pdf-save` uses the mapping
//! its own page replay produced.

pub mod edit;
pub mod encoding;
pub mod error;
pub mod insert;
pub mod parse;

#[cfg(test)]
mod fixture;

pub use edit::{
    image_source_bytes, move_image, remove_image, remove_text_run, replace_image_source,
    replace_text_run, resize_image,
};
pub use error::EditError;
pub use insert::{insert_image, insert_text_run};
pub use parse::{page_object_id, read_page_content};
