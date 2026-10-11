// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Form fields through the generated UniFFI API.
extension UniFfiPdfCoreClient {
    func formFields(document: any PdfDocument) -> [FormField] {
        guard let handle = try? handle(of: document) else { return [] }
        return handle.listFormFields().map(FormField.init)
    }

    func formFieldEditingAllowed(document: any PdfDocument) -> Bool {
        (try? handle(of: document).formFieldEditingAllowed()) ?? false
    }

    func setFieldValue(document: any PdfDocument, fieldId: UInt64, value: FieldValue) throws {
        let handle = try handle(of: document)
        try Self.mapEditErrors { try applyEdit(handle: handle, command: .setFieldValue(fieldId: fieldId, value: FfiFieldValue(value))) }
    }
}

private extension FormField {
    init(_ field: FfiFormField) {
        let kind: FormFieldKind
        switch field.kind {
        case let .text(multiline, maxLen): kind = .text(multiline: multiline, maxLength: maxLen.map(Int.init))
        case .checkbox: kind = .checkbox
        case let .radioGroup(options): kind = .radioGroup(options: options.map(\.exportValue))
        case let .dropdown(options, editable): kind = .dropdown(options: options, editable: editable)
        }
        self.init(
            id: field.id, page: Int(field.page), name: field.name,
            rect: PdfRect(x: field.rect.x, y: field.rect.y, width: field.rect.width, height: field.rect.height),
            kind: kind, value: FieldValue(field.value)
        )
    }
}

private extension FieldValue {
    init(_ value: FfiFieldValue) {
        switch value {
        case let .text(text): self = .text(text)
        case let .checked(checked): self = .checked(checked)
        case let .choice(option): self = .choice(option)
        }
    }
}

private extension FfiFieldValue {
    init(_ value: FieldValue) {
        switch value {
        case let .text(text): self = .text(text: text)
        case let .checked(checked): self = .checked(checked: checked)
        case let .choice(option): self = .choice(option: option)
        }
    }
}
