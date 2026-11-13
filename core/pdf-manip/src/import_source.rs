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
use crate::open::open_document_from_bytes_admitted;
use crate::security::text_extraction_is_allowed;

/// Opens `bytes` as an import source, refusing a document whose `/P` forbids
/// copying or extracting its content.
///
/// Returns the decrypted document together with the security context its
/// open resolved — including for a document whose user password is empty,
/// which opens without a prompt yet still carries real permissions (see
/// `open::security_from_probe`). The bytes are parsed once for the security
/// context and, only when that admits the import and the document is still
/// encrypted, once more to decrypt it: a refused source never reaches the
/// decrypting load.
///
/// A missing or wrong password is reported as [`ManipError::WrongPassword`]
/// (the open probes a missing one as the empty password, which only an
/// unlocked document matches) and never folded into the permission refusal:
/// a shell prompts for a password, and that is not the same answer as "this
/// PDF forbids it".
pub fn open_import_source_from_bytes(
    bytes: &[u8],
    credential: Option<&str>,
) -> Result<(LopdfDocument, Option<SecurityContext>), ManipError> {
    open_document_from_bytes_admitted(bytes, credential, |security| {
        if text_extraction_is_allowed(security) {
            Ok(())
        } else {
            Err(ManipError::SourceForbidsCopying)
        }
    })
}
