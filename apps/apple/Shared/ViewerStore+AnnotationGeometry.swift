// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Typing where the selected annotation goes and how big it is — Windows'
/// `MainWindow.AnnotationPosition.cs` / `MainWindow.AnnotationSize.cs` and
/// Android's precise dimensions (#304), over the same core move and resize
/// a drag records.
extension ViewerStore {
    enum AnnotationGeometry {
        static let positionHint = "Position the annotation's bottom-left bounds in unrotated PDF space. X increases rightward; Y increases upward. Size stays unchanged."
        static let sizeHint = "The annotation stays at its current PDF-space origin. Width and height are independent; stamps stretch to fit."
        static let positionUnchanged = "Annotation position unchanged."
        static let sizeUnchanged = "Annotation dimensions unchanged."
        static let invalidPosition = "Enter a finite X and Y in points."
        static let invalidSize = "Width and height must be finite and greater than zero."
        static let noRect = "This annotation has no box to resize."
    }

    /// Moves the selected annotation so its bounds' bottom-left corner sits
    /// at `x`, `y`, keeping its size. Returns why the sheet must stay open,
    /// or `nil` once it is done — including "unchanged", which is said on
    /// the status line, as Windows does.
    func moveSelectedAnnotation(toX x: Double, y: Double) -> String? {
        guard annotationEditingAllowed, let selected = selectedAnnotation, let bounds = selected.bounds else { return nil }
        let dx = x - bounds.x, dy = y - bounds.y
        guard x.isFinite, y.isFinite, dx.isFinite, dy.isFinite else { return AnnotationGeometry.invalidPosition }
        guard dx != 0 || dy != 0 else {
            editStatus = AnnotationGeometry.positionUnchanged
            return nil
        }
        return applyAnnotationEdit(.move(id: selected.id, dx: dx, dy: dy), status: "Annotation moved. Changes are pending save.")
    }

    /// Gives the selected annotation `width` × `height` points from its
    /// current origin. Ink has no box to replace — Linux's `supports_resize`.
    func resizeSelectedAnnotation(width: Double, height: Double) -> String? {
        guard annotationEditingAllowed, let selected = selectedAnnotation else { return nil }
        guard let rect = selected.rect else { return AnnotationGeometry.noRect }
        guard width.isFinite, height.isFinite, width > 0, height > 0 else { return AnnotationGeometry.invalidSize }
        // The floor a corner drag already keeps a text box above.
        if selected.kind == .freeText, width < Self.freeTextMinimumSize.width || height < Self.freeTextMinimumSize.height {
            return "A text box must be at least \(Int(Self.freeTextMinimumSize.width)) × \(Int(Self.freeTextMinimumSize.height)) points."
        }
        guard width != rect.width || height != rect.height else {
            editStatus = AnnotationGeometry.sizeUnchanged
            return nil
        }
        let resized = PdfRect(x: rect.x, y: rect.y, width: width, height: height)
        return applyAnnotationEdit(.resize(id: selected.id, rect: resized), status: "Annotation resized. Changes are pending save.")
    }
}
