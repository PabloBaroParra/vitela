// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
//
// Form field value types — mirrors of the generated `FfiFormField*` and
// `FfiFieldValue`, so `ViewerStore` never sees a UniFFI type.
import Foundation

enum FieldValue: Equatable {
    case text(String)
    case checked(Bool)
    /// A radio button's or dropdown's choice; `nil` is none.
    case choice(String?)
}

enum FormFieldKind: Equatable {
    /// `maxLength` caps the text; zero means read-only.
    case text(multiline: Bool, maxLength: Int?)
    case checkbox
    case radioGroup(options: [String])
    /// An editable dropdown also accepts text outside its options.
    case dropdown(options: [String], editable: Bool)
}

/// One AcroForm field, as the core lists it.
struct FormField: Equatable {
    let id: UInt64
    let page: Int
    let name: String
    let rect: PdfRect
    let kind: FormFieldKind
    let value: FieldValue
}

/// What a dropdown row shows for "no choice", and how its rows map to
/// choices — Windows' `FormFieldChoices`.
enum FormFieldChoices {
    static let noChoiceLabel = "(none)"

    /// The row that shows `choice`: a value outside `options` — possible in
    /// a file written by another tool — shows as no choice.
    static func index(of choice: String?, in options: [String]) -> Int {
        guard let choice, let index = options.firstIndex(of: choice) else { return 0 }
        return index + 1
    }

    static func choice(at index: Int, in options: [String]) -> String? {
        index >= 1 && index <= options.count ? options[index - 1] : nil
    }
}

/// Defaults for clients without form support.
extension PdfCoreClient {
    func formFields(document: any PdfDocument) -> [FormField] { [] }
    func formFieldEditingAllowed(document: any PdfDocument) -> Bool { false }

    func setFieldValue(document: any PdfDocument, fieldId: UInt64, value: FieldValue) throws {
        throw EditFailure.failed("this client does not support forms")
    }
}
