//! `pdf-manip`: lopdf-backed merge/split/rotate/reorder/extract/create-blank/
//! insert-remove-page and decrypt-on-open operations — see Batch 4
//! (T-022..T-026) and `design.md`.
//!
//! `lopdf` is this crate's own dependency and is never re-exported as part
//! of `pdf_document`'s public surface — `pdf_document` has no I/O deps at
//! all (see that crate's docs). Everything crossing *this* crate's public
//! API uses [`LopdfDocument`] (an opaque wrapper this crate owns) or
//! `pdf_document`'s own pure-data types (`PageSize`, `Orientation`,
//! `SecurityContext`, ...) — never a bare `lopdf::Document`/`lopdf::Error`.
//!
//! The Batch 0 port traits were intentionally retired before B7 because their
//! placeholder signatures did not match these operations' real contracts.

mod create_blank;
mod document;
mod error;
mod merge_split;
mod open;
mod page_ops;
mod security;

pub use create_blank::{create_blank_document, insert_blank_page, remove_page};
pub use document::{LopdfDocument, PageDimensions};
pub use error::ManipError;
pub use merge_split::{merge, split};
pub use open::{
    open_document, open_document_from_bytes, open_document_with_passwords,
    open_document_with_passwords_from_bytes, read_security_context,
    read_security_context_from_bytes,
};
pub use page_ops::{delete_pages, extract_pages, reorder_pages, rotate_page};
pub use security::text_extraction_is_allowed;
