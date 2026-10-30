// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// A press on a vector path in Edit Content: its body moves it, a corner of
/// the selected path reshapes it — Windows' `ContentPathDrag`.
struct PathDrag: Equatable {
    let path: ContentPath
    let origin: PdfPoint
    var current: PdfPoint
    let corner: AnnotationCorner?

    /// Where the path would be if released here. An axis the path is flat
    /// along (a rule's height) stays flat and in place: the core cannot
    /// stretch nothing — Windows' `PathDragRect`.
    var previewRect: PdfRect {
        let bounds = path.bbox
        guard let corner else {
            return PdfRect(x: bounds.x + current.x - origin.x, y: bounds.y + current.y - origin.y,
                           width: bounds.width, height: bounds.height)
        }
        let resized = ViewerStore.resizedRect(bounds, corner: corner, to: current)
        return PdfRect(x: bounds.width == 0 ? bounds.x : resized.x,
                       y: bounds.height == 0 ? bounds.y : resized.y,
                       width: bounds.width == 0 ? 0 : resized.width,
                       height: bounds.height == 0 ? 0 : resized.height)
    }
}

extension ContentEditing {
    static let pathSelected = "Shape selected — drag it, drag a corner to resize, or press Delete."

    /// Whether a press at `point` means `path`: anywhere on a filled shape,
    /// but only near the edges of an outline — inside a frame by more than
    /// `reach` is its cell, not its border. Windows' `PathHit`.
    static func pathHit(_ path: ContentPath, at point: PdfPoint, reach: Double) -> Bool {
        let bounds = path.bbox
        let padded = PdfRect(x: bounds.x - reach, y: bounds.y - reach, width: bounds.width + 2 * reach, height: bounds.height + 2 * reach)
        guard padded.contains(point) else { return false }
        guard path.isOutline else { return true }
        let inner = PdfRect(x: bounds.x + reach, y: bounds.y + reach, width: bounds.width - 2 * reach, height: bounds.height - 2 * reach)
        return inner.width <= 0 || inner.height <= 0 || !inner.contains(point)
    }

    /// The path a press meant: the selected one while the press is on it or
    /// its corner, so an overhanging handle still reshapes it; otherwise the
    /// smallest hit — a rule inside a frame inside a shaded panel is the one
    /// a reader aiming at it means. Windows' `PathAt`.
    static func path(at point: PdfPoint, in paths: [ContentPath], selected: ContentPath?, reach: Double) -> ContentPath? {
        if let selected, ViewerStore.corner(of: selected.bbox, at: point, reach: reach) != nil || pathHit(selected, at: point, reach: reach) {
            return selected
        }
        return paths.filter { pathHit($0, at: point, reach: reach) }
            .min { $0.bbox.width * $0.bbox.height < $1.bbox.width * $1.bbox.height }
    }
}

/// Selecting, moving, reshaping and deleting a vector path (B25 T-206) —
/// Windows' `MainWindow.PathGestures.cs` over `PdfDocumentFacade.Paths.cs`.
/// Unlike an image there is no "save and reopen first" rule: the core reads
/// a pending move back at its new box and folds a second geometry edit into
/// the queued one, so the path is selected again after each edit.
extension ViewerStore {
    var selectedContentPath: ContentPath? { contentEdit.selectedPath }

    var pathDrag: PathDrag? { contentEdit.pathDrag }

    /// Grabs the path under `point` on `page`, selecting it; a press on the
    /// selected path's corner reshapes it. A miss drops the selection.
    func beginPathDrag(page: Int, at point: PdfPoint, zoom: Double) -> Bool {
        let reach = Self.handleReach / max(zoom, 0.01)
        let selected = contentEdit.selectedPath.flatMap { $0.page == page ? $0 : nil }
        guard let paths = pageContent[page]?.paths,
              let hit = ContentEditing.path(at: point, in: paths, selected: selected, reach: reach)
        else {
            contentEdit.selectedPath = nil
            return false
        }
        let corner = hit == selected ? Self.corner(of: hit.bbox, at: point, reach: reach) : nil
        contentEdit.selectedImage = nil
        contentEdit.selectedPath = hit
        contentEdit.pathDrag = PathDrag(path: hit, origin: point, current: point, corner: corner)
        editStatus = ContentEditing.pathSelected
        return true
    }

    func continuePathDrag(to point: PdfPoint) {
        contentEdit.pathDrag?.current = point
    }

    /// A release that left the path where it was only selects it.
    func endPathDrag(at point: PdfPoint) -> ContentRelease {
        continuePathDrag(to: point)
        guard let drag = contentEdit.pathDrag else { return .nothing }
        contentEdit.pathDrag = nil
        let to = drag.previewRect
        guard to != drag.path.bbox else { return .nothing }
        let edit: ContentEdit = drag.corner == nil ? .movePath(drag.path, to: to) : .resizePath(drag.path, to: to)
        guard let request = beginContentEdit(edit) else { return .nothing }
        contentEdit.reselectPath = ContentImageKey(page: drag.path.page, id: drag.path.id)
        return .move(request)
    }

    /// The selected path's Delete.
    func beginSelectedPathDelete() -> PageWorkRequest? {
        guard contentEditMode, let path = contentEdit.selectedPath else { return nil }
        contentEdit.reselectPath = nil
        return beginContentEdit(.removePath(path))
    }

    /// Called with each page read: the path an edit just landed on is
    /// selected again at its new box, once.
    func reselectPath(on page: Int, in content: PageContent) {
        guard contentEditMode, let key = contentEdit.reselectPath, key.page == page else { return }
        contentEdit.reselectPath = nil
        contentEdit.selectedPath = content.paths.first { $0.id == key.id }
    }
}
