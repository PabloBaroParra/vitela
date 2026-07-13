//! AuditLog: append-only, non-undoable security/consent record (T-013).
//!
//! Per spec "Encrypted Document Save Behavior": the explicit-strip-consent
//! event MUST be recorded separately from the undoable `EditLog` — protection
//! removal must never be reversible via undo (Ctrl+Z). Keeping `AuditLog` a
//! wholly distinct structure (not a `Command` variant) makes that guarantee
//! structural rather than a rule callers must remember to honor: `EditLog`
//! undo/redo has no code path that can even see, let alone roll back, an
//! `AuditEvent`.

/// A security/consent-relevant event, recorded independently of document
/// content edits.
///
/// `#[non_exhaustive]`: future audit-worthy events (e.g. a digital-signature
/// application, per the signatures scope-change) can be added without a
/// breaking change — costs nothing today.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum AuditEvent {
    /// User explicitly chose "Remove protection" and confirmed, per spec
    /// scenario "Explicit strip with consent". Saved output will be
    /// unencrypted; this event is the durable record that it was
    /// user-initiated, not an accidental/implicit strip.
    StripProtectionConsent,
}

/// Who triggered the audited event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditActor {
    User,
    System,
}

/// A single recorded audit event with its actor.
#[derive(Debug, Clone, PartialEq)]
pub struct AuditEntry {
    pub event: AuditEvent,
    pub actor: AuditActor,
}

/// Append-only audit trail, separate from `EditLog`. There is intentionally
/// no `undo`/`redo` on this type — audit entries are permanent.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AuditLog {
    entries: Vec<AuditEntry>,
}

impl AuditLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an event to the audit trail. There is no corresponding
    /// "undo" by design.
    pub fn record(&mut self, event: AuditEvent, actor: AuditActor) {
        self.entries.push(AuditEntry { event, actor });
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Document, PageId};
    use crate::edit_log::Command;

    #[test]
    fn audit_log_records_strip_protection_consent() {
        let mut audit_log = AuditLog::new();
        audit_log.record(AuditEvent::StripProtectionConsent, AuditActor::User);

        assert_eq!(audit_log.len(), 1);
        assert_eq!(audit_log.entries()[0].actor, AuditActor::User);
        assert_eq!(
            audit_log.entries()[0].event,
            AuditEvent::StripProtectionConsent
        );
    }

    /// Spec "Strip is not undoable": recording a strip-consent event MUST
    /// NOT touch the EditLog, and undoing all EditLog entries MUST NOT
    /// affect the audit log — they are structurally independent.
    #[test]
    fn audit_log_is_independent_of_edit_log_undo() {
        let mut document = Document::blank();

        document
            .audit_log
            .record(AuditEvent::StripProtectionConsent, AuditActor::User);
        assert_eq!(document.audit_log.len(), 1);
        assert!(document.pending_edits.entries().is_empty());

        // Undoing an empty EditLog is a no-op and must not reach the audit
        // log at all. (`mem::take` avoids double-borrowing `document` while
        // calling a method on one of its own fields.)
        let mut edit_log = std::mem::take(&mut document.pending_edits);
        let undone = edit_log.undo(&mut document);
        assert!(!undone);
        document.pending_edits = edit_log;
        assert_eq!(document.audit_log.len(), 1);

        // Even with real content edits recorded and then undone, the audit
        // log entry recorded earlier must remain untouched.
        use crate::annotation::{Annotation, AnnotationId, AnnotationKind, Color, Rect};
        let annotation = Annotation {
            id: AnnotationId(1),
            page: PageId(0),
            kind: AnnotationKind::Highlight {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                },
                color: Color { r: 0, g: 0, b: 0 },
            },
        };
        let mut edit_log = std::mem::take(&mut document.pending_edits);
        edit_log.apply(&mut document, Command::AddAnnotation(annotation));
        edit_log.undo(&mut document);
        document.pending_edits = edit_log;

        assert_eq!(document.audit_log.len(), 1);
        assert!(document.annotations.is_empty());
    }
}
