//! Opening a PDF as the *source* of an import.
//!
//! A source is not just another document to open: before a single page of it
//! is decrypted, its own permissions have to allow being copied out of. That
//! rule lived in the Linux GTK4 shell, which meant it was a rule only that
//! shell honoured — every other shell would have had to remember to write it
//! again, and the core would have happily grafted a page out of a document
//! that forbids exactly that.
//!
//! So the gate lives here, next to the open it gates, and the shells lose the
//! choice (checklist "Pruebas del núcleo", `docs/batch-pdf-assembly.md`
//! section 12).

use pdf_document::SecurityContext;

use crate::document::LopdfDocument;
use crate::error::ManipError;
use crate::open::{open_document_from_bytes, read_security_context_from_bytes};
use crate::security::text_extraction_is_allowed;

/// Opens `bytes` as an import source, refusing a document whose `/P` forbids
/// copying or extracting its content.
///
/// Returns the decrypted document together with the security context the
/// *probe* read, which is the honest one: a document whose user password is
/// empty is decrypted in place by lopdf's unauthenticated load, so it looks
/// unencrypted to [`open_document_from_bytes`] while still carrying real
/// permissions (see `open::security_from_probe`). Gating on what that load
/// reports would wave through the most common restricted PDF there is.
///
/// A missing or wrong password is reported as
/// [`ManipError::PasswordRequired`] / [`ManipError::WrongPassword`] and never
/// folded into the permission refusal: a shell prompts on the first and gives
/// up on the second, and they are not the same answer.
pub fn open_import_source_from_bytes(
    bytes: &[u8],
    credential: Option<&str>,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    let security = read_security_context_from_bytes(bytes, credential)?;
    if !text_extraction_is_allowed(security.as_ref()) {
        return Err(ManipError::SourceForbidsCopying);
    }
    let (document, _) = open_document_from_bytes(bytes, credential)?;
    Ok((document, security))
}
