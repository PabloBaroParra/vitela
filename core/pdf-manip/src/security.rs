//! Permission policy: what an open document's `/Encrypt` permissions
//! actually allow (spec "Open Password-Protected PDF").
//!
//! `pdf_document::Permissions` deliberately stores the raw `/P` bitmask and
//! nothing else — "bit semantics are owned by the encryption layer
//! (`pdf-manip`); this crate stores the raw value only" (see
//! `pdf_document::security`). This module is that layer, and it is the single
//! place a permission bit is interpreted: the FFI boundary reads it for every
//! shell that crosses UniFFI, and the GTK4 shell — which links the core
//! crates directly and bypasses that boundary — reads the same function, so a
//! restricted document behaves identically on every platform.

use pdf_document::{Credential, SecurityContext};

/// `/P` bit 5 (0-indexed bit 4, `lopdf::Permissions::COPYABLE`) per PDF 1.7
/// table 22: "copy or otherwise extract text and graphics from the document".
///
/// Not to be confused with bit 10 (`COPYABLE_FOR_ACCESSIBILITY`), which
/// covers extraction *in support of accessibility* — a document routinely
/// grants that while forbidding plain copying, so gating on bit 10 would let
/// exactly the documents this check exists for through.
const COPY_OR_EXTRACT: u32 = 1 << 4;
const ANNOTATE: u32 = 1 << 5;
/// `/P` bit 4 (0-indexed bit 3) per PDF 1.7 table 22: "modify the contents of
/// the document by operations other than those controlled by" the other
/// content-related bits. This is the general edit-the-page-content
/// permission — distinct from bit 6's narrower "add or modify annotations"
/// (T-161: editing an existing text run or image is a content-stream change,
/// not an annotation, so it is gated on this bit instead).
const MODIFY_CONTENTS: u32 = 1 << 3;

/// Whether the document permits copying/extracting its text.
///
/// `None` — an unencrypted document — permits everything. An owner-credential
/// open bypasses the bitmask: the owner password authenticates the party that
/// set the restrictions in the first place.
pub fn text_extraction_is_allowed(security: Option<&SecurityContext>) -> bool {
    match security {
        Some(security) => {
            security.credential == Credential::Owner
                || security.permissions.0 & COPY_OR_EXTRACT != 0
        }
        None => true,
    }
}

/// Whether the document permits changes to annotation objects.
///
/// PDF `/P` bit 6 grants adding or modifying annotations. This is distinct
/// from bit 4's general document-modification permission. As with text
/// extraction, an owner credential bypasses the user permission bitmask and
/// unencrypted documents permit it.
pub fn annotation_editing_is_allowed(security: Option<&SecurityContext>) -> bool {
    match security {
        Some(security) => {
            security.credential == Credential::Owner || security.permissions.0 & ANNOTATE != 0
        }
        None => true,
    }
}

/// Whether the document permits changes to page content (text runs, images).
///
/// As with [`annotation_editing_is_allowed`], an owner credential bypasses
/// the bitmask and unencrypted documents permit it. A document may grant one
/// of the annotate/modify-contents bits without the other, so this must not
/// be inferred from `annotation_editing_is_allowed`.
pub fn content_editing_is_allowed(security: Option<&SecurityContext>) -> bool {
    match security {
        Some(security) => {
            security.credential == Credential::Owner
                || security.permissions.0 & MODIFY_CONTENTS != 0
        }
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{EncryptionCredentials, Permissions, SecurityHandler};

    fn context(credential: Credential, permissions: u32) -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(permissions),
        }
    }

    #[test]
    fn an_unencrypted_document_permits_extraction() {
        assert!(text_extraction_is_allowed(None));
    }

    #[test]
    fn a_user_open_follows_the_copy_bit() {
        assert!(!text_extraction_is_allowed(Some(&context(
            Credential::User,
            0
        ))));
        assert!(text_extraction_is_allowed(Some(&context(
            Credential::User,
            COPY_OR_EXTRACT
        ))));
    }

    #[test]
    fn an_owner_open_bypasses_the_copy_bit() {
        assert!(text_extraction_is_allowed(Some(&context(
            Credential::Owner,
            0
        ))));
    }

    #[test]
    fn the_accessibility_bit_alone_does_not_permit_copying() {
        // Bit 10: extraction in support of accessibility. Very common on
        // documents that forbid plain copying — it must not open the gate.
        assert!(!text_extraction_is_allowed(Some(&context(
            Credential::User,
            1 << 9
        ))));
    }

    #[test]
    fn a_user_open_follows_the_annotation_bit_for_annotation_edits() {
        assert!(!annotation_editing_is_allowed(Some(&context(
            Credential::User,
            0
        ))));
        assert!(annotation_editing_is_allowed(Some(&context(
            Credential::User,
            ANNOTATE
        ))));
    }

    #[test]
    fn a_user_open_requires_the_annotation_bit_not_the_modify_bit() {
        assert!(!annotation_editing_is_allowed(Some(&context(
            Credential::User,
            1 << 3
        ))));
        assert!(annotation_editing_is_allowed(Some(&context(
            Credential::User,
            ANNOTATE
        ))));
    }

    #[test]
    fn an_owner_open_may_edit_annotations_without_the_modify_bit() {
        assert!(annotation_editing_is_allowed(Some(&context(
            Credential::Owner,
            0
        ))));
    }

    #[test]
    fn a_user_open_follows_the_modify_contents_bit_for_content_edits() {
        assert!(!content_editing_is_allowed(Some(&context(
            Credential::User,
            0
        ))));
        assert!(content_editing_is_allowed(Some(&context(
            Credential::User,
            MODIFY_CONTENTS
        ))));
    }

    /// The annotate bit alone must not open the content-edit gate — a
    /// document can grant one and withhold the other.
    #[test]
    fn the_annotate_bit_alone_does_not_permit_content_edits() {
        assert!(!content_editing_is_allowed(Some(&context(
            Credential::User,
            ANNOTATE
        ))));
    }

    #[test]
    fn an_owner_open_may_edit_content_without_the_modify_bit() {
        assert!(content_editing_is_allowed(Some(&context(
            Credential::Owner,
            0
        ))));
    }

    #[test]
    fn an_unencrypted_document_permits_content_edits() {
        assert!(content_editing_is_allowed(None));
    }
}
