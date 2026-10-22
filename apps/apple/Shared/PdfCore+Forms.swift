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

/// The standard fonts a field's text can use — mirrors `FfiFontFamily`.
enum FormFont: CaseIterable, Equatable {
    case helvetica, timesRoman, courier

    var name: String {
        switch self {
        case .helvetica: return "Helvetica"
        case .timesRoman: return "Times"
        case .courier: return "Courier"
        }
    }
}

/// How a field's text is drawn — mirrors `FfiTextStyle`.
struct FormTextStyle: Equatable {
    var font: FormFont
    var sizePt: Double
    var color: PdfColor

    /// What every shell gives a new field.
    static let `default` = FormTextStyle(font: .helvetica, sizePt: 12, color: PdfColor(r: 0, g: 0, b: 0))

    /// The sizes a field's text may take — Linux's `Adjustment(12, 4, 400)`
    /// and Windows' `FormStyleSize` (`Minimum="4" Maximum="400"`).
    static let sizeRange: ClosedRange<Double> = 4...400

    /// A typed size brought inside `sizeRange`, as both spin controls coerce
    /// one; `nil` for a value that is no size at all.
    static func clampedSize(_ value: Double) -> Double? {
        value.isFinite ? min(max(value, sizeRange.lowerBound), sizeRange.upperBound) : nil
    }
}

/// One AcroForm field, as the core lists it.
struct FormField: Equatable {
    let id: UInt64
    let page: Int
    let name: String
    let rect: PdfRect
    let kind: FormFieldKind
    let value: FieldValue
    var style: FormTextStyle = .default
}

/// One change to a field's structure — the form subset of `FfiEditCommand`.
enum FieldEdit: Equatable {
    case move(id: UInt64, to: PdfRect)
    case resize(id: UInt64, to: PdfRect)
    case remove(id: UInt64)
    case rename(id: UInt64, name: String)
    case restyle(id: UInt64, style: FormTextStyle)

    /// What the status line says once it is recorded — Windows' wording.
    var done: String {
        switch self {
        case .move: return "Field moved."
        case .resize: return "Field resized."
        case .remove: return "Field deleted."
        case .rename: return "Field renamed."
        case .restyle: return "Field style changed."
        }
    }
}

/// The kinds of field the Form panel can place — Windows' `FieldToPlace`.
enum FieldToPlace: CaseIterable, Equatable {
    case text, checkbox, radioGroup, dropdown

    var name: String {
        switch self {
        case .text: return "Text field"
        case .checkbox: return "Checkbox"
        case .radioGroup: return "Radio group"
        case .dropdown: return "Dropdown"
        }
    }

    /// The box a click places, in points — Windows' and Linux's sizes.
    var clickSize: (width: Double, height: Double) {
        switch self {
        case .checkbox: return (18, 18)
        case .radioGroup: return (144, 48)
        case .text, .dropdown: return (144, 36)
        }
    }
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

    func addField(document: any PdfDocument, kind: FieldToPlace, page: Int, rect: PdfRect) throws {
        throw EditFailure.failed("this client does not support forms")
    }

    func applyFieldEdit(document: any PdfDocument, edit: FieldEdit) throws {
        throw EditFailure.failed("this client does not support forms")
    }
}
