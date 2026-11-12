//! Whether a document lets an edit be recorded at all — the permission and
//! rewrite questions every boundary asks before an edit joins the log.
//!
//! This used to live only in `pdf-ffi`'s `apply_edit`, so a boundary that
//! does not cross UniFFI (`pdf-wasm`) would have had to copy it. It needs
//! nothing a boundary owns: the document's [`SecurityContext`] and what kind
//! of edit is being asked for.
//!
//! ## Why the edit is described, not passed
//!
//! The gate runs **before** a boundary builds its [`Command`]: building one
//! can mint a page id or allocate an annotation id, and it can fail on a bad
//! position. A refused edit must leave both counters alone, and a document
//! that forbids the edit must say so even when the position is also wrong.
//! So the gate takes an [`EditClass`] — what the edit is — which a boundary
//! can know before it builds anything. [`EditClass::of`] derives one from a
//! built [`Command`] with `pdf_document`'s own predicates, for a boundary
//! that builds first and for checking a boundary's own classification.

use pdf_document::{Command, SecurityContext};

use crate::rewrite::{full_rewrite_blocker, RewriteBlocker};

/// What kind of edit is about to be recorded — the questions the gate asks
/// about it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditClass {
    /// Edits an annotation or a form field (`/P` bit 6, the floor for every
    /// form command, filling one in included).
    pub annotation: bool,
    /// Changes which pages the document has, in what order, or how they are
    /// turned (`/P` bit 11).
    pub document_assembly: bool,
    /// Creates, removes, moves, resizes, restyles or renames a form field —
    /// everything but filling one in (`/P` bit 4 on top of bit 6).
    pub structural_form: bool,
    /// Rewrites a page's own content: text runs, images, paths, watermarks
    /// (`/P` bit 4), and forces the full-rewrite writer.
    pub content: bool,
    /// Changes which pages the document has or in what order, forcing the
    /// full-rewrite writer. Narrower than `document_assembly` by exactly the
    /// rotations, which stay on the incremental writer.
    pub page_structure: bool,
}

impl EditClass {
    /// The class of a built `command`, read with `pdf_document`'s own
    /// predicates.
    pub fn of(command: &Command) -> Self {
        let document_assembly = command.is_document_assembly_edit();
        let content = command.is_content_edit();
        EditClass {
            // Everything that is not page structure, page content or the
            // document's metadata is an annotation or a form field.
            annotation: !(document_assembly
                || content
                || matches!(command, Command::SetDocumentInfo { .. })),
            document_assembly,
            structural_form: command.is_form_field_edit()
                && !matches!(command, Command::SetFieldValue { .. }),
            content,
            page_structure: command.is_page_structure_edit(),
        }
    }
}

/// Why the gate refused an edit. Its `Display` is the reader-facing message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditRefusal {
    /// The document withholds the annotate permission.
    AnnotationEditing,
    /// The document withholds the document-assembly permission.
    DocumentAssembly,
    /// The document withholds the modify-contents permission a structural
    /// form edit needs.
    FormFieldEditing,
    /// The document withholds the modify-contents permission.
    ContentEditing,
    /// The edit forces a full rewrite that this document's encryption cannot
    /// survive.
    FullRewrite(RewriteBlocker),
}

impl std::fmt::Display for EditRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EditRefusal::AnnotationEditing => f.write_str("annotation editing is not permitted"),
            EditRefusal::DocumentAssembly => f.write_str(
                "this document does not permit inserting, removing or rotating its pages",
            ),
            EditRefusal::FormFieldEditing => {
                f.write_str("this document does not permit creating or modifying its form fields")
            }
            EditRefusal::ContentEditing => f.write_str("content editing is not permitted"),
            EditRefusal::FullRewrite(blocker) => write!(
                f,
                "this edit rewrites the whole file, which this document's encryption does not \
                 allow: {}",
                blocker.reason()
            ),
        }
    }
}

impl std::error::Error for EditRefusal {}

/// Refuses an edit of class `class` on a document opened with `security`, or
/// lets it through.
///
/// The questions are asked in a fixed order and the first refusal wins:
/// annotation, assembly, structural form, content, then the rewrite.
///
/// - A structural form edit is not excluded from the annotation question:
///   bit 6 is the floor for every form command. What it needs on top is
///   bit 4, and only bit 4 — a form edit never forces the full-rewrite
///   writer, so it is not asked the rewrite question. Asking would refuse
///   field editing on an AES-256 document that saves incrementally.
/// - Page-content and page-structure edits both force the full-rewrite
///   writer, so both are asked whether that writer could reproduce the
///   encryption. A rotation is not: it stays on the incremental writer,
///   which re-encrypts from lopdf's own retained state. Refusing here rather
///   than at the save means an edit that can never be written is never
///   queued — which would also break the preview refresh, which saves the
///   same way.
pub fn check_edit(security: Option<&SecurityContext>, class: EditClass) -> Result<(), EditRefusal> {
    if class.annotation && !pdf_manip::annotation_editing_is_allowed(security) {
        return Err(EditRefusal::AnnotationEditing);
    }
    if class.document_assembly && !pdf_manip::document_assembly_is_allowed(security) {
        return Err(EditRefusal::DocumentAssembly);
    }
    if class.structural_form && !pdf_manip::content_editing_is_allowed(security) {
        return Err(EditRefusal::FormFieldEditing);
    }
    if class.content && !pdf_manip::content_editing_is_allowed(security) {
        return Err(EditRefusal::ContentEditing);
    }
    if class.content || class.page_structure {
        if let Some(blocker) = full_rewrite_blocker(security) {
            return Err(EditRefusal::FullRewrite(blocker));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdf_document::{
        Credential, EncryptionCredentials, FieldValue, FormFieldId, Orientation, Page, PageId,
        PageSize, Permissions, SecurityHandler,
    };

    const PRINT: u32 = 1 << 2;
    const MODIFY_CONTENTS: u32 = 1 << 3;
    const ANNOTATE: u32 = 1 << 5;
    const ASSEMBLE: u32 = 1 << 10;

    /// An RC4 document opened with its user password only, granting
    /// `permissions` — the shape every refusal below needs.
    fn user_open(permissions: u32) -> SecurityContext {
        SecurityContext {
            handler: SecurityHandler::Rc4_128,
            credential: Credential::User,
            credentials: EncryptionCredentials::default(),
            permissions: Permissions(permissions),
        }
    }

    /// Both passwords known, so a full rewrite could re-encrypt it.
    fn rewritable(permissions: u32) -> SecurityContext {
        let mut security = user_open(permissions);
        security.credentials = EncryptionCredentials {
            user_password: Some("user".to_string()),
            owner_password: Some("owner".to_string()),
        };
        security
    }

    fn class(set: impl FnOnce(&mut EditClass)) -> EditClass {
        let mut class = EditClass::default();
        set(&mut class);
        class
    }

    #[test]
    fn an_unencrypted_document_lets_every_edit_through() {
        let everything = EditClass {
            annotation: true,
            document_assembly: true,
            structural_form: true,
            content: true,
            page_structure: true,
        };

        assert_eq!(check_edit(None, everything), Ok(()));
    }

    #[test]
    fn an_annotation_needs_the_annotate_bit() {
        let annotation = class(|c| c.annotation = true);

        assert_eq!(
            check_edit(Some(&user_open(PRINT)), annotation),
            Err(EditRefusal::AnnotationEditing)
        );
        assert_eq!(check_edit(Some(&user_open(ANNOTATE)), annotation), Ok(()));
    }

    #[test]
    fn a_rotation_needs_the_assembly_bit_but_not_a_rewrite() {
        let rotation = class(|c| c.document_assembly = true);

        assert_eq!(
            check_edit(Some(&user_open(PRINT)), rotation),
            Err(EditRefusal::DocumentAssembly)
        );
        // One password only: a rewrite is impossible, a rotation still fine.
        assert_eq!(check_edit(Some(&user_open(ASSEMBLE)), rotation), Ok(()));
    }

    #[test]
    fn a_page_structure_edit_also_needs_a_possible_rewrite() {
        let removal = class(|c| {
            c.document_assembly = true;
            c.page_structure = true;
        });

        assert_eq!(
            check_edit(Some(&user_open(ASSEMBLE)), removal),
            Err(EditRefusal::FullRewrite(
                RewriteBlocker::IncompleteCredentials
            ))
        );
        assert_eq!(check_edit(Some(&rewritable(ASSEMBLE)), removal), Ok(()));
    }

    /// The permission is asked before the rewrite: a document that forbids
    /// the edit says so, not that it could not be written.
    #[test]
    fn the_assembly_refusal_wins_over_the_rewrite_refusal() {
        let removal = class(|c| {
            c.document_assembly = true;
            c.page_structure = true;
        });

        assert_eq!(
            check_edit(Some(&user_open(PRINT)), removal),
            Err(EditRefusal::DocumentAssembly)
        );
    }

    #[test]
    fn a_structural_form_edit_needs_both_bits_and_no_rewrite() {
        let new_field = class(|c| {
            c.annotation = true;
            c.structural_form = true;
        });

        assert_eq!(
            check_edit(Some(&user_open(MODIFY_CONTENTS)), new_field),
            Err(EditRefusal::AnnotationEditing)
        );
        assert_eq!(
            check_edit(Some(&user_open(ANNOTATE)), new_field),
            Err(EditRefusal::FormFieldEditing)
        );
        assert_eq!(
            check_edit(Some(&user_open(ANNOTATE | MODIFY_CONTENTS)), new_field),
            Ok(())
        );
    }

    #[test]
    fn a_content_edit_needs_the_modify_bit_and_a_possible_rewrite() {
        let retype = class(|c| c.content = true);

        assert_eq!(
            check_edit(Some(&user_open(ANNOTATE)), retype),
            Err(EditRefusal::ContentEditing)
        );
        assert_eq!(
            check_edit(Some(&user_open(MODIFY_CONTENTS)), retype),
            Err(EditRefusal::FullRewrite(
                RewriteBlocker::IncompleteCredentials
            ))
        );
        assert_eq!(
            check_edit(Some(&rewritable(MODIFY_CONTENTS)), retype),
            Ok(())
        );
    }

    /// The words every boundary shows, kept exactly as `pdf-ffi` wrote them.
    #[test]
    fn each_refusal_reads_as_before() {
        assert_eq!(
            EditRefusal::AnnotationEditing.to_string(),
            "annotation editing is not permitted"
        );
        assert_eq!(
            EditRefusal::DocumentAssembly.to_string(),
            "this document does not permit inserting, removing or rotating its pages"
        );
        assert_eq!(
            EditRefusal::FormFieldEditing.to_string(),
            "this document does not permit creating or modifying its form fields"
        );
        assert_eq!(
            EditRefusal::ContentEditing.to_string(),
            "content editing is not permitted"
        );
        assert_eq!(
            EditRefusal::FullRewrite(RewriteBlocker::IncompleteCredentials).to_string(),
            format!(
                "this edit rewrites the whole file, which this document's encryption does not \
                 allow: {}",
                RewriteBlocker::IncompleteCredentials.reason()
            )
        );
    }

    fn blank(id: u32) -> Page {
        Page::blank(PageId(id), PageSize::A4, Orientation::Portrait)
    }

    #[test]
    fn the_class_of_a_rotation_is_assembly_only() {
        let rotation = Command::RotatePages {
            pages: vec![PageId(0)],
            delta_degrees: 90,
        };

        assert_eq!(
            EditClass::of(&rotation),
            class(|c| c.document_assembly = true)
        );
    }

    #[test]
    fn the_class_of_a_page_move_is_assembly_and_structure() {
        let page_edits = [
            Command::MovePages {
                from: 0,
                count: 1,
                to: 1,
            },
            Command::InsertPage {
                index: 0,
                page: blank(9),
                annotations: Vec::new(),
                form_fields: Vec::new(),
            },
            Command::ImportPages {
                index: 0,
                pages: vec![blank(9)],
            },
        ];

        for command in page_edits {
            assert_eq!(
                EditClass::of(&command),
                class(|c| {
                    c.document_assembly = true;
                    c.page_structure = true;
                }),
                "{command:?}"
            );
        }
    }

    #[test]
    fn the_class_of_a_fill_is_annotation_only() {
        let fill = Command::SetFieldValue {
            id: FormFieldId(0),
            from: FieldValue::Text(String::new()),
            to: FieldValue::Text("x".to_string()),
        };

        assert_eq!(EditClass::of(&fill), class(|c| c.annotation = true));
    }

    #[test]
    fn the_class_of_document_info_asks_nothing() {
        let info = Command::SetDocumentInfo {
            before: Default::default(),
            after: Default::default(),
        };

        assert_eq!(EditClass::of(&info), EditClass::default());
    }
}
