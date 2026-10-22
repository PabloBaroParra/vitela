// Editing what the session placed: select an annotation with a click, drag
// its body to move it or a corner handle to resize it, and delete, nudge,
// grow or recolor the selection. Mirrors Windows' `MainWindow.Annotations.cs`
// (`TryGrabSelected`, `ResizedRect`, Nudge/Grow) and the Linux
// `annotations/edit.rs`, with their constants.
import Foundation

/// Named by page-space position, where `y` grows upward — Linux's `Corner`.
enum AnnotationCorner: CaseIterable, Equatable {
    case bottomLeft, bottomRight, topLeft, topRight

    var opposite: AnnotationCorner {
        switch self {
        case .bottomLeft: return .topRight
        case .bottomRight: return .topLeft
        case .topLeft: return .bottomRight
        case .topRight: return .bottomLeft
        }
    }

    func point(of rect: PdfRect) -> PdfPoint {
        switch self {
        case .bottomLeft: return PdfPoint(x: rect.x, y: rect.y)
        case .bottomRight: return PdfPoint(x: rect.x + rect.width, y: rect.y)
        case .topLeft: return PdfPoint(x: rect.x, y: rect.y + rect.height)
        case .topRight: return PdfPoint(x: rect.x + rect.width, y: rect.y + rect.height)
        }
    }
}

/// A selected annotation being dragged: by its body (`corner == nil`) to
/// move it, or by a corner handle to resize it.
struct AnnotationGesture: Equatable {
    let annotation: Annotation
    let origin: PdfPoint
    var current: PdfPoint
    let corner: AnnotationCorner?

    /// Where the annotation would be if the drag ended here.
    var previewRect: PdfRect? {
        guard let bounds = annotation.rect else { return nil }
        if let corner {
            // A text box may not shrink below what the core accepts.
            return annotation.kind == .freeText
                ? ViewerStore.freeTextResizedRect(anchor: corner.opposite.point(of: bounds), pointer: current)
                : ViewerStore.resizedRect(bounds, corner: corner, to: current)
        }
        return PdfRect(x: bounds.x + dx, y: bounds.y + dy, width: bounds.width, height: bounds.height)
    }

    var dx: Double { current.x - origin.x }
    var dy: Double { current.y - origin.y }
}

extension ViewerStore {
    /// How far Nudge shifts the selection — Linux's `NUDGE_PT`.
    static let nudgePoints = 12.0
    /// How much Grow enlarges the selection — Linux's `RESIZE_FACTOR`.
    static let growFactor = 1.25
    /// Corner-handle size and grab radius in screen points — `HANDLE_PX`.
    static let handleReach = 8.0

    var selectedAnnotation: Annotation? {
        guard let id = selectedAnnotationId else { return nil }
        return annotations.last { $0.id == id }
    }

    /// A press on `page` with no tool armed. The selected annotation gets
    /// first refusal, so its handles stay reachable where another overlaps
    /// them; otherwise the topmost annotation under the point is selected
    /// and grabbed. A press on nothing deselects and is not claimed, so the
    /// shell goes on to select text. `zoom` sizes the handles' reach.
    func beginAnnotationGesture(page: Int, at point: PdfPoint, zoom: Double) -> Bool {
        guard armedTool == nil, annotationEditingAllowed else { return false }
        if let selected = selectedAnnotation, selected.page == page, let bounds = selected.bounds {
            let reach = Self.handleReach / max(zoom, 0.01)
            if selected.rect != nil, let corner = Self.corner(of: bounds, at: point, reach: reach) {
                annotationGesture = AnnotationGesture(annotation: selected, origin: point, current: point, corner: corner)
                return true
            }
            if bounds.contains(point) {
                annotationGesture = AnnotationGesture(annotation: selected, origin: point, current: point, corner: nil)
                return true
            }
        }
        guard let hit = annotations.last(where: { $0.page == page && ($0.bounds?.contains(point) ?? false) }) else {
            selectedAnnotationId = nil
            return false
        }
        selectedAnnotationId = hit.id
        annotationGesture = AnnotationGesture(annotation: hit, origin: point, current: point, corner: nil)
        return true
    }

    func continueAnnotationGesture(to point: PdfPoint) {
        annotationGesture?.current = point
    }

    /// Commits the move or resize as one undoable edit — none when the
    /// pointer came back where it started.
    func endAnnotationGesture(at point: PdfPoint) {
        guard var gesture = annotationGesture else { return }
        gesture.current = point
        annotationGesture = nil
        let id = gesture.annotation.id
        if gesture.corner != nil, let resized = gesture.previewRect {
            if resized != gesture.annotation.rect { applyAnnotationEdit(.resize(id: id, rect: resized)) }
        } else if gesture.dx != 0 || gesture.dy != 0 {
            applyAnnotationEdit(.move(id: id, dx: gesture.dx, dy: gesture.dy))
        }
    }

    func cancelAnnotationGesture() {
        annotationGesture = nil
    }

    func deleteSelectedAnnotation() {
        guard annotationEditingAllowed, let selected = selectedAnnotation else { return }
        applyAnnotationEdit(.remove(id: selected.id))
        selectedAnnotationId = nil
    }

    func nudgeSelectedAnnotation() {
        guard annotationEditingAllowed, let selected = selectedAnnotation else { return }
        applyAnnotationEdit(.move(id: selected.id, dx: Self.nudgePoints, dy: Self.nudgePoints))
    }

    /// Ink has no rect to replace, so it cannot grow — Linux's `supports_resize`.
    func growSelectedAnnotation() {
        guard annotationEditingAllowed, let selected = selectedAnnotation, let rect = selected.rect else { return }
        let grown = PdfRect(x: rect.x, y: rect.y, width: rect.width * Self.growFactor, height: rect.height * Self.growFactor)
        applyAnnotationEdit(.resize(id: selected.id, rect: grown))
    }

    func restyleSelectedAnnotation(_ color: PdfColor) {
        guard let id = selectedAnnotationId else { return }
        restyleAnnotation(id: id, to: color)
    }

    /// Recolors annotation `id`, selected or not — a color that settles
    /// after the reader has moved on still lands on the annotation it was
    /// picked for. Gone, unrestylable or unchanged: nothing, and no undo step.
    func restyleAnnotation(id: UInt64, to color: PdfColor) {
        guard annotationEditingAllowed, let target = annotations.last(where: { $0.id == id }), target.kind.supportsRestyle,
              target.color != color
        else { return }
        applyAnnotationEdit(.restyle(id: id, color: color), status: "Annotation restyled. Changes are pending save.")
    }

    /// The corner whose handle is under `point`, if any — Linux's `corner_at`.
    static func corner(of rect: PdfRect, at point: PdfPoint, reach: Double) -> AnnotationCorner? {
        AnnotationCorner.allCases.first { corner in
            let at = corner.point(of: rect)
            return abs(point.x - at.x) <= reach && abs(point.y - at.y) <= reach
        }
    }

    /// The grabbed corner follows the pointer and the opposite one stays put;
    /// dragging past the opposite corner flips the rect rather than inverting
    /// it — Linux's `resized_rect`.
    static func resizedRect(_ rect: PdfRect, corner: AnnotationCorner, to point: PdfPoint) -> PdfRect {
        let anchor = corner.opposite.point(of: rect)
        return PdfRect(
            x: min(anchor.x, point.x),
            y: min(anchor.y, point.y),
            width: max(minimumTracedPoints, abs(point.x - anchor.x)),
            height: max(minimumTracedPoints, abs(point.y - anchor.y))
        )
    }
}
