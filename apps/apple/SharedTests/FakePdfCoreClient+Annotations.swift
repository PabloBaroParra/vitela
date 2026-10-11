// The annotation, comment, stamp and form doubles of `FakePdfCoreClient`.
// Each records into the fake's one (Info, layout, annotations, field values)
// history, so undo and redo cover them like the core's EditLog does.
import Foundation
@testable import Vitela

extension FakePdfCoreClient {
    func annotations(document: any PdfDocument) -> [Annotation] { currentAnnotations }
    func annotationEditingAllowed(document: any PdfDocument) -> Bool { annotationsPermitted }

    /// The file's own comments, then this session's notes — as the core merges them.
    func comments(document: any PdfDocument) -> [Comment] {
        (savedComments + currentAnnotations.filter { $0.kind == .textNote }.map {
            Comment(page: $0.page, rect: $0.rect ?? PdfRect(x: 0, y: 0, width: 0, height: 0),
                    contents: $0.contents ?? "", author: nil, date: nil, annotationId: $0.id)
        }).sorted { $0.page < $1.page }
    }

    func applyAnnotationEdit(document: any PdfDocument, edit: AnnotationEdit) throws {
        if let annotationEditError { throw annotationEditError }
        annotationEdits.append(edit)
        var list = currentAnnotations
        switch edit {
        case let .add(kind, page, rect, color):
            list.append(Annotation(id: takeAnnotationId(), page: page, kind: kind, rect: rect, points: [], color: color, contents: nil))
        case let .addInk(page, points, color):
            list.append(Annotation(id: takeAnnotationId(), page: page, kind: .ink, rect: nil, points: points, color: color, contents: nil))
        case let .addNote(page, rect, contents):
            list.append(Annotation(id: takeAnnotationId(), page: page, kind: .textNote, rect: rect, points: [], color: nil, contents: contents))
        case let .remove(id):
            list.removeAll { $0.id == id }
        case let .move(id, dx, dy):
            list = list.map { a in
                guard a.id == id else { return a }
                return Annotation(
                    id: a.id, page: a.page, kind: a.kind,
                    rect: a.rect.map { PdfRect(x: $0.x + dx, y: $0.y + dy, width: $0.width, height: $0.height) },
                    points: a.points.map { PdfPoint(x: $0.x + dx, y: $0.y + dy) }, color: a.color, contents: a.contents
                )
            }
        case let .resize(id, rect):
            list = list.map { a in
                a.id == id ? Annotation(id: a.id, page: a.page, kind: a.kind, rect: rect, points: a.points, color: a.color, contents: a.contents) : a
            }
        case let .restyle(id, color):
            list = list.map { a in
                a.id == id ? Annotation(id: a.id, page: a.page, kind: a.kind, rect: a.rect, points: a.points, color: color, contents: a.contents) : a
            }
        case let .addFreeText(page, rect, contents):
            list.append(Annotation(
                id: takeAnnotationId(), page: page, kind: .freeText, rect: rect, points: [], color: nil, contents: contents,
                freeTextLayout: FreeTextLayout(fontSizePt: 12, lines: [.init(text: contents, xPt: 2, baselineFromTopPt: 12)], overflow: false)
            ))
        case let .setContents(id, contents):
            list = list.map { a in
                a.id == id ? Annotation(id: a.id, page: a.page, kind: a.kind, rect: a.rect, points: a.points, color: a.color, contents: contents, freeTextLayout: a.freeTextLayout) : a
            }
        }
        recordAnnotations(list)
    }

    func insertImageStamp(document: any PdfDocument, page: Int, imageBytes: Data, rect: PdfRect) throws {
        if let annotationEditError { throw annotationEditError }
        stampInserts.append((page, rect))
        recordAnnotations(currentAnnotations + [
            Annotation(id: takeAnnotationId(), page: page, kind: .stamp, rect: rect, points: [], color: nil, contents: nil),
        ])
    }

    /// A 2:1 image, 100 points wide, hanging from the anchor.
    func stampPlacement(imageBytes: Data, anchor: PdfPoint) throws -> PdfRect {
        PdfRect(x: anchor.x, y: anchor.y - 50, width: 100, height: 50)
    }

    var currentFieldValues: [UInt64: FieldValue] {
        historyCursor == 0 ? [:] : history[historyCursor - 1].fieldValues
    }

    func formFields(document: any PdfDocument) -> [FormField] {
        let values = currentFieldValues
        return fields.map { field in
            FormField(id: field.id, page: field.page, name: field.name, rect: field.rect, kind: field.kind, value: values[field.id] ?? field.value, style: field.style)
        }
    }

    func formFieldEditingAllowed(document: any PdfDocument) -> Bool { fieldStructurePermitted }

    func applyFieldEdit(document: any PdfDocument, edit: FieldEdit) throws {
        if let fieldFillError { throw fieldFillError }
        fieldEdits.append(edit)
        func replace(_ id: UInt64, _ change: (FormField) -> FormField) {
            fields = fields.map { $0.id == id ? change($0) : $0 }
        }
        switch edit {
        case let .move(id, rect), let .resize(id, rect):
            replace(id) { FormField(id: $0.id, page: $0.page, name: $0.name, rect: rect, kind: $0.kind, value: $0.value, style: $0.style) }
        case let .remove(id):
            fields.removeAll { $0.id == id }
        case let .rename(id, name):
            replace(id) { FormField(id: $0.id, page: $0.page, name: name, rect: $0.rect, kind: $0.kind, value: $0.value, style: $0.style) }
        case let .restyle(id, style):
            replace(id) { FormField(id: $0.id, page: $0.page, name: $0.name, rect: $0.rect, kind: $0.kind, value: $0.value, style: style) }
        }
    }

    func addField(document: any PdfDocument, kind: FieldToPlace, page: Int, rect: PdfRect) throws {
        if let fieldFillError { throw fieldFillError }
        let fieldKind: FormFieldKind
        let value: FieldValue
        switch kind {
        case .text: (fieldKind, value) = (.text(multiline: false, maxLength: nil), .text(""))
        case .checkbox: (fieldKind, value) = (.checkbox, .checked(false))
        case .radioGroup: (fieldKind, value) = (.radioGroup(options: ["Option 1", "Option 2"]), .choice(nil))
        case .dropdown: (fieldKind, value) = (.dropdown(options: ["Option 1", "Option 2"], editable: false), .choice(nil))
        }
        addedFields.append(kind)
        fields.append(FormField(id: UInt64(1000 + fields.count), page: page, name: "Field \(fields.count + 1)", rect: rect, kind: fieldKind, value: value))
    }

    func setFieldValue(document: any PdfDocument, fieldId: UInt64, value: FieldValue) throws {
        if let fieldFillError { throw fieldFillError }
        fills.append((fieldId, value))
        var values = currentFieldValues
        values[fieldId] = value
        history = Array(history.prefix(historyCursor)) + [Snapshot(
            info: (try? documentInfo(document: FakeCoreDocument(pages: pages))) ?? initialInfo,
            pages: currentPages, annotations: currentAnnotations, fieldValues: values
        )]
        historyCursor = history.count
    }

    func recordAnnotations(_ list: [Annotation]) {
        history = Array(history.prefix(historyCursor))
            + [Snapshot(info: (try? documentInfo(document: FakeCoreDocument(pages: pages))) ?? initialInfo, pages: currentPages, annotations: list, fieldValues: currentFieldValues)]
        historyCursor = history.count
    }

    func takeAnnotationId() -> UInt64 {
        defer { nextAnnotationId += 1 }
        return nextAnnotationId
    }
}
