// Placing a new form field on a page. Mirrors Windows'
// `MainWindow.FormPlacement.cs` and `Viewer/FormPlacementRect.cs`: arm a
// field kind in the Form panel, then a click places the default box hanging
// from the click point, or a drag places the box it drew. The field's
// appearance is painted by the PDF, so the creation is page work.
import Foundation

/// A field placement being traced.
struct FieldDraft: Equatable {
    let kind: FieldToPlace
    let page: Int
    let origin: PdfPoint
    var current: PdfPoint
}

extension ViewerStore {
    func armField(_ kind: FieldToPlace?) {
        guard document != nil else { return }
        guard kind == nil || formStructureAllowed else {
            formStatus = "This document does not permit creating form fields."
            return
        }
        armedField = kind
        fieldDraft = nil
        if kind != nil {
            // One mode owns a page click at a time.
            armedTool = nil
            selectedAnnotationId = nil
        }
        formStatus = kind.map { "Click or drag on a page to place a \($0.name.lowercased())." } ?? ""
    }

    func beginFieldDraft(page: Int, at point: PdfPoint) -> Bool {
        guard let kind = armedField, pageSlots.indices.contains(page) else { return false }
        fieldDraft = FieldDraft(kind: kind, page: page, origin: point, current: point)
        return true
    }

    func continueFieldDraft(page: Int, to point: PdfPoint) {
        guard fieldDraft?.page == page else { return }
        fieldDraft?.current = point
    }

    /// The page work that creates the traced field, or `nil` when there is
    /// nothing to place. Disarms the kind, as Windows does after a placement.
    func endFieldDraft(page: Int, at point: PdfPoint) -> PageWorkRequest? {
        guard let draft = fieldDraft, draft.page == page, let document, !pageWorkInFlight else {
            fieldDraft = nil
            return nil
        }
        fieldDraft = nil
        armedField = nil
        let rect = Self.fieldPlacementRect(
            origin: draft.origin, end: point, page: pageSlots[page].dimensions, clickSize: draft.kind.clickSize
        )
        guard rect.width > 0, rect.height > 0 else { return nil }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .addField(kind: draft.kind, page: page, rect: rect))
    }

    /// Windows' `FormPlacementRect.Resolve`: both points clamped to the page
    /// (in its unturned space); a press that moved less than 8 points hangs
    /// the click-size box from the press, clamped onto the page; a drag
    /// places what it drew, at least 4 points a side.
    static func fieldPlacementRect(origin: PdfPoint, end: PdfPoint, page: PageDimensions, clickSize: (width: Double, height: Double)) -> PdfRect {
        let (width, height) = unrotatedSize(page)
        guard width > 0, height > 0 else { return PdfRect(x: 0, y: 0, width: 0, height: 0) }
        func clamp(_ value: Double, _ upper: Double) -> Double { min(max(value, 0), upper) }
        let startX = clamp(origin.x, width), startY = clamp(origin.y, height)
        let endX = clamp(end.x, width), endY = clamp(end.y, height)
        let dx = endX - startX, dy = endY - startY
        if dx * dx + dy * dy < 64 {
            let boxWidth = min(clickSize.width, width), boxHeight = min(clickSize.height, height)
            return PdfRect(
                x: clamp(startX, width - boxWidth),
                y: clamp(startY - boxHeight, height - boxHeight),
                width: boxWidth,
                height: boxHeight
            )
        }
        let x = min(startX, endX), y = min(startY, endY)
        return PdfRect(x: x, y: y, width: min(max(4, abs(dx)), width - x), height: min(max(4, abs(dy)), height - y))
    }
}
