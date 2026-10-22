// Changing the fields a form already has: Edit fields mode, where a click on
// a field selects it, dragging its body moves it and a corner handle resizes
// it, and the Form panel deletes, renames and restyles it or takes exact
// coordinates. Mirrors Windows' `MainWindow.FormToolbar.cs`,
// `FormGestures.cs`, `FormDelete.cs` and `FormNames.cs`. Each change is one
// undoable edit; the PDF paints fields, so each is page work.
import Foundation

/// A selected field being dragged by its body or by a corner handle.
struct FieldGesture: Equatable {
    let field: FormField
    let origin: PdfPoint
    var current: PdfPoint
    let corner: AnnotationCorner?

    /// Where the field would be if the drag ended here. A grabbed corner
    /// moves by the pointer's delta, not to the pointer — the handle is hit
    /// anywhere within its reach, so snapping would resize on a plain click.
    var previewRect: PdfRect {
        let dx = current.x - origin.x, dy = current.y - origin.y
        guard let corner else {
            return PdfRect(x: field.rect.x + dx, y: field.rect.y + dy, width: field.rect.width, height: field.rect.height)
        }
        let grabbed = corner.point(of: field.rect)
        return ViewerStore.resizedRect(field.rect, corner: corner, to: PdfPoint(x: grabbed.x + dx, y: grabbed.y + dy))
    }
}

extension ViewerStore {
    var selectedField: FormField? {
        guard let id = selectedFieldId else { return nil }
        return formFields.first { $0.id == id }
    }

    /// T-143's panel→page half: a fill-panel control gaining focus selects
    /// its field on the page, so the outline shows which field is being
    /// filled — Linux's `fill::mark_selected`, Windows' row `GotFocus`. Only in
    /// Edit fields mode, so tabbing through the panel while just filling in
    /// values never grabs a selection; a field already selected, or gone, is
    /// left alone.
    func selectFieldFromPanel(_ id: UInt64) {
        guard formEditMode, selectedFieldId != id, formFields.contains(where: { $0.id == id }) else { return }
        selectedFieldId = id
    }

    func setFormEditMode(_ on: Bool) {
        guard document != nil, !on || formStructureAllowed else { return }
        formEditMode = on
        fieldGesture = nil
        if on {
            // One mode owns a page click at a time.
            armedTool = nil
            contentEditMode = false
            selectedAnnotationId = nil
            formStatus = "Edit fields armed — pick a field type to place, or click an existing field to select it."
        } else {
            armedField = nil
            selectedFieldId = nil
            formStatus = "Edit fields disarmed."
        }
    }

    /// A press on `page` in Edit fields mode with no kind armed. The selected
    /// field gets first refusal for its handles; otherwise the topmost field
    /// under the point is selected and grabbed; a press on nothing deselects.
    /// Claims the press whenever the mode is on, so text is not selected.
    func beginFieldGesture(page: Int, at point: PdfPoint, zoom: Double) -> Bool {
        guard formEditMode, armedField == nil else { return false }
        let selected = selectedField.flatMap { $0.page == page ? $0 : nil }
        let corner = selected.flatMap { Self.corner(of: $0.rect, at: point, reach: Self.handleReach / max(zoom, 0.01)) }
        let hit: FormField?
        if let selected, corner != nil || selected.rect.contains(point) {
            hit = selected
        } else {
            hit = formFields.last { $0.page == page && $0.rect.contains(point) }
        }
        guard let hit else {
            selectedFieldId = nil
            return true
        }
        selectedFieldId = hit.id
        fieldGesture = FieldGesture(field: hit, origin: point, current: point, corner: hit.id == selected?.id ? corner : nil)
        return true
    }

    func continueFieldGesture(to point: PdfPoint) {
        fieldGesture?.current = point
    }

    /// The move or resize as page work, or `nil` when the field ended where
    /// it started.
    func endFieldGesture(at point: PdfPoint) -> PageWorkRequest? {
        guard var gesture = fieldGesture else { return nil }
        gesture.current = point
        fieldGesture = nil
        let after = gesture.previewRect
        guard after != gesture.field.rect else { return nil }
        return beginFieldEdit(gesture.corner == nil ? .move(id: gesture.field.id, to: after) : .resize(id: gesture.field.id, to: after))
    }

    /// Exact coordinates typed in the panel; unchanged ones are no edit.
    func beginFieldGeometryEdit(x: Double, y: Double, width: Double, height: Double) -> PageWorkRequest? {
        guard let field = selectedField else { return nil }
        guard [x, y, width, height].allSatisfy(\.isFinite), width > 0, height > 0 else {
            formStatus = "Enter finite field coordinates."
            return nil
        }
        let rect = field.rect
        if rect.width != width || rect.height != height {
            return beginFieldEdit(.resize(id: field.id, to: PdfRect(x: x, y: y, width: width, height: height)))
        }
        guard rect.x != x || rect.y != y else { return nil }
        return beginFieldEdit(.move(id: field.id, to: PdfRect(x: x, y: y, width: width, height: height)))
    }

    func beginFieldRename(_ name: String) -> PageWorkRequest? {
        guard let field = selectedField, name != field.name else { return nil }
        return beginFieldEdit(.rename(id: field.id, name: name))
    }

    func beginFieldRestyle(_ style: FormTextStyle) -> PageWorkRequest? {
        guard let field = selectedField, style != field.style else { return nil }
        return beginFieldEdit(.restyle(id: field.id, style: style))
    }

    /// Field `id`'s text in `color`, selected or not — a color that settles
    /// in the panel after the selection moved still lands on its own field.
    /// An unchanged color is no edit, so no undo step.
    func beginFieldColor(id: UInt64, color: PdfColor) -> PageWorkRequest? {
        guard let field = formFields.first(where: { $0.id == id }), field.style.color != color else { return nil }
        var style = field.style
        style.color = color
        return beginFieldEdit(.restyle(id: id, style: style))
    }

    func beginFieldDelete() -> PageWorkRequest? {
        guard let field = selectedField else { return nil }
        selectedFieldId = nil
        return beginFieldEdit(.remove(id: field.id))
    }

    private func beginFieldEdit(_ edit: FieldEdit) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight else { return nil }
        guard formStructureAllowed else {
            formStatus = "This document does not permit changing its form fields."
            return nil
        }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .editField(edit))
    }
}
