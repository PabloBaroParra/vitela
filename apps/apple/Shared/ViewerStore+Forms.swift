// Filling in the AcroForm fields a document already has. Mirrors Windows'
// `MainWindow.Forms.cs` and the Linux `forms/fill.rs`: one control per field,
// built from the core's list, never from the page; each fill is one undoable
// edit. Nothing is drawn over the page — the PDF paints a field's value from
// its appearance — so a fill re-derives the render side like a page edit and
// goes through the same page-work pipeline.
import Foundation

extension ViewerStore {
    /// Starts filling `fieldId` with `value`, or returns `nil` when there is
    /// nothing to do: the value is the one the field already holds (tabbing
    /// through a field must not leave an undo step), the field is gone, or
    /// the document refuses — then `formStatus` says why.
    func beginFieldFill(_ fieldId: UInt64, value: FieldValue) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight else { return nil }
        guard formFillAllowed else {
            formStatus = "This document does not permit filling in its form."
            return nil
        }
        guard let field = formFields.first(where: { $0.id == fieldId }) else {
            formStatus = "The document changed. Please try again."
            return nil
        }
        guard field.value != value else { return nil }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .fillField(id: fieldId, value: value))
    }

    /// Re-reads the core's field list — on open, after a fill, a page edit,
    /// an undo or a redo.
    func refreshFormFields() {
        formFields = document.map { client.formFields(document: $0) } ?? []
        formStatus = formFields.isEmpty || formFillAllowed ? "" : "This document does not permit filling in its form."
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetForms() {
        formFillAllowed = annotationEditingAllowed
        formStructureAllowed = document.map { client.formFieldEditingAllowed(document: $0) } ?? false
        refreshFormFields()
    }
}

/// How a text field's value travels between the core and a text control: the
/// core stores `\n`; a control may hand back `\r\n` or `\r` — Windows'
/// `FormFieldText`.
enum FormFieldText {
    static func fromControl(_ text: String) -> String {
        text.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
    }

    /// Typed text cut to the field's cap, so a control never offers a value
    /// the core would refuse.
    static func capped(_ text: String, maxLength: Int?) -> String {
        guard let maxLength else { return text }
        return String(text.prefix(max(maxLength, 0)))
    }
}
