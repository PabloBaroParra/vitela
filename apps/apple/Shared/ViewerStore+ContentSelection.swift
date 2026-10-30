// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// What a marquee or a SHIFT+click gathered on one page (B25 T-208): text
/// blocks, images and paths — Windows' `ContentSelection`. A "table" is what
/// a marquee over it gathers: its border paths and the text inside them.
struct ContentSelection: Equatable {
    let page: Int
    private(set) var blocks: [ContentTextBlock] = []
    private(set) var images: [ContentImage] = []
    private(set) var paths: [ContentPath] = []

    init(page: Int) { self.page = page }

    var count: Int { blocks.count + images.count + paths.count }
    var isEmpty: Bool { count == 0 }

    /// The union of the members' boxes.
    var bbox: PdfRect? {
        let boxes = blocks.map(\.bbox) + images.map(\.bbox) + paths.map(\.bbox)
        guard let first = boxes.first else { return nil }
        return boxes.dropFirst().reduce(first, Self.union)
    }

    func contains(_ block: ContentTextBlock) -> Bool { blocks.contains { $0.id == block.id } }
    func contains(_ image: ContentImage) -> Bool { images.contains { $0.id == image.id } }
    func contains(_ path: ContentPath) -> Bool { paths.contains { $0.id == path.id } }

    func toggled(_ block: ContentTextBlock) -> ContentSelection {
        var next = self
        if contains(block) { next.blocks.removeAll { $0.id == block.id } } else { next.blocks.append(block) }
        return next
    }

    func toggled(_ image: ContentImage) -> ContentSelection {
        var next = self
        if contains(image) { next.images.removeAll { $0.id == image.id } } else { next.images.append(image) }
        return next
    }

    func toggled(_ path: ContentPath) -> ContentSelection {
        var next = self
        if contains(path) { next.paths.removeAll { $0.id == path.id } } else { next.paths.append(path) }
        return next
    }

    /// `self` plus whatever on `content` a marquee over `rect` touches — a
    /// member whose box intersects it, as decided for T-208.
    func gathering(_ content: PageContent, in rect: PdfRect) -> ContentSelection {
        var next = self
        let blocks = content.textBlocks.isEmpty ? content.textRuns.map(ContentTextBlock.init) : content.textBlocks
        for block in blocks where Self.touches(block.bbox, rect) && !next.contains(block) { next.blocks.append(block) }
        for image in content.images where Self.touches(image.bbox, rect) && !next.contains(image) { next.images.append(image) }
        for path in content.paths where Self.touches(path.bbox, rect) && !next.contains(path) { next.paths.append(path) }
        return next
    }

    /// The same members as they now read on `content`, after a move: by id,
    /// at their new boxes; a member no longer there is dropped.
    func reread(from content: PageContent) -> ContentSelection {
        var next = ContentSelection(page: page)
        for block in blocks {
            if let run = content.textRuns.first(where: { $0.id == block.first.id }) { next.blocks.append(content.block(containing: run)) }
        }
        next.images = images.compactMap { image in content.images.first { $0.id == image.id } }
        next.paths = paths.compactMap { path in content.paths.first { $0.id == path.id } }
        return next
    }

    static func touches(_ a: PdfRect, _ b: PdfRect) -> Bool {
        a.x <= b.x + b.width && b.x <= a.x + a.width && a.y <= b.y + b.height && b.y <= a.y + a.height
    }

    static func union(_ a: PdfRect, _ b: PdfRect) -> PdfRect {
        let x = min(a.x, b.x), y = min(a.y, b.y)
        return PdfRect(x: x, y: y, width: max(a.x + a.width, b.x + b.width) - x, height: max(a.y + a.height, b.y + b.height) - y)
    }
}

/// A press that a multi-selection owns: a marquee from empty page, or a
/// drag of everything selected — Windows' `SelectionGesture`.
struct SelectionGesture: Equatable {
    enum Kind { case marquee, drag }
    let kind: Kind
    let page: Int
    let origin: PdfPoint
    var current: PdfPoint
    let zoom: Double
    /// What a SHIFT-marquee adds to; `nil` for a plain one.
    let base: ContentSelection?
    var dragging = false

    /// The marquee's rectangle, PDF space.
    var rect: PdfRect {
        PdfRect(x: min(origin.x, current.x), y: min(origin.y, current.y),
                width: abs(current.x - origin.x), height: abs(current.y - origin.y))
    }
}

extension ContentEditing {
    static let nothingToSelect = "Nothing there to select."
    static let selectionHint = "drag inside the dashed box to move them, press Delete to remove them, Shift-click to add or remove one."

    static func selected(_ count: Int) -> String {
        "\(count) \(count == 1 ? "item" : "items") selected — \(selectionHint)"
    }
}

/// Multi-item selection in Edit Content (B25 T-208) — Windows'
/// `MainWindow.ContentSelection.cs` over `move_content_selection` /
/// `remove_content_selection`, each one undo step. The single-item
/// selections stay as they were; at most one of the two is ever set.
extension ViewerStore {
    var contentSelection: ContentSelection? { contentEdit.multiSelection }

    var selectionGesture: SelectionGesture? { contentEdit.selectionGesture }

    /// Claims the press for a selection gesture, or returns `false` so the
    /// single-item gestures take it: with `extend` (SHIFT) it toggles the
    /// item under the press, or starts a marquee that adds; a press inside
    /// the selection drags it; a press on empty page starts a marquee. A
    /// press near a line keeps opening it, as before T-208.
    func beginSelectionGesture(page: Int, at point: PdfPoint, zoom: Double, extend: Bool, content: PageContent) -> Bool {
        let reach = Self.handleReach / max(zoom, 0.01)
        // A selected image's or shape's corner overhangs its box: a resize.
        if let image = contentEdit.selectedImage, image.page == page, Self.corner(of: image.bbox, at: point, reach: reach) != nil { return false }
        if let path = contentEdit.selectedPath, path.page == page, Self.corner(of: path.bbox, at: point, reach: reach) != nil { return false }

        let hitImage = content.images.last { $0.bbox.contains(point) }
        let hitBlock = ContentEditing.textRun(at: point, in: content.textRuns, reach: 0).map(content.block(containing:))
        let hitPath = hitImage == nil && hitBlock == nil
            ? ContentEditing.path(at: point, in: content.paths, selected: contentEdit.selectedPath, reach: reach) : nil

        if extend {
            let current = selectionToExtend(page: page)
            if let hitImage {
                setContentSelection(current.toggled(hitImage))
            } else if let hitBlock {
                setContentSelection(current.toggled(hitBlock))
            } else if let hitPath {
                setContentSelection(current.toggled(hitPath))
            } else {
                setContentSelection(current.isEmpty ? nil : current)
                contentEdit.selectionGesture = SelectionGesture(kind: .marquee, page: page, origin: point, current: point,
                                                                zoom: max(zoom, 0.01), base: current)
            }
            return true
        }
        if let selection = contentEdit.multiSelection, selection.page == page, let bbox = selection.bbox,
           PdfRect(x: bbox.x - reach, y: bbox.y - reach, width: bbox.width + 2 * reach, height: bbox.height + 2 * reach).contains(point) {
            contentEdit.selectionGesture = SelectionGesture(kind: .drag, page: page, origin: point, current: point,
                                                            zoom: max(zoom, 0.01), base: nil)
            return true
        }
        contentEdit.multiSelection = nil
        if hitImage != nil || hitBlock != nil || hitPath != nil { return false }
        if textRun(page: page, at: point, zoom: zoom) != nil { return false }
        contentEdit.selectedImage = nil
        contentEdit.selectedPath = nil
        contentEdit.selectionGesture = SelectionGesture(kind: .marquee, page: page, origin: point, current: point,
                                                        zoom: max(zoom, 0.01), base: nil)
        return true
    }

    func continueSelectionGesture(to point: PdfPoint) {
        guard var gesture = contentEdit.selectionGesture else { return }
        gesture.current = point
        let travel = max(abs(point.x - gesture.origin.x), abs(point.y - gesture.origin.y)) * gesture.zoom
        if travel >= ContentEditing.clickEpsilon { gesture.dragging = true }
        contentEdit.selectionGesture = gesture
    }

    func endSelectionGesture(at point: PdfPoint) -> ContentRelease {
        continueSelectionGesture(to: point)
        guard let gesture = contentEdit.selectionGesture else { return .nothing }
        contentEdit.selectionGesture = nil
        switch gesture.kind {
        case .marquee:
            guard gesture.dragging else {
                // A plain click on empty page says what a click there does.
                if gesture.base == nil { editStatus = ContentEditing.missed }
                return .nothing
            }
            guard let content = pageContent[gesture.page] else { return .nothing }
            let selection = (gesture.base ?? ContentSelection(page: gesture.page)).gathering(content, in: gesture.rect)
            setContentSelection(selection.isEmpty ? nil : selection)
            if selection.isEmpty { editStatus = ContentEditing.nothingToSelect }
            return .nothing
        case .drag:
            let dx = gesture.current.x - gesture.origin.x, dy = gesture.current.y - gesture.origin.y
            guard gesture.dragging, dx != 0 || dy != 0, let selection = contentEdit.multiSelection,
                  let request = beginContentEdit(.moveSelection(selection, dx: dx, dy: dy))
            else { return .nothing }
            contentEdit.reselectSelection = selection
            return .move(request)
        }
    }

    /// Delete with a multi-selection: every member goes, one undo step.
    func beginSelectionDelete() -> PageWorkRequest? {
        guard contentEditMode, let selection = contentEdit.multiSelection else { return nil }
        contentEdit.reselectSelection = nil
        return beginContentEdit(.removeSelection(selection))
    }

    func clearContentSelection() {
        contentEdit.multiSelection = nil
        contentEdit.selectionGesture = nil
    }

    /// After a move, the same members are selected where they landed, so
    /// the next drag or Delete acts on them — Windows' `keep: true`.
    func reselectContentSelection(on page: Int, in content: PageContent) {
        guard contentEditMode, let selection = contentEdit.reselectSelection, selection.page == page else { return }
        contentEdit.reselectSelection = nil
        let reread = selection.reread(from: content)
        // The status line keeps saying what the move did.
        setContentSelection(reread.isEmpty ? nil : reread, announce: false)
    }

    /// Makes `selection` the multi-selection and lets go of any single-item
    /// one; `nil` clears it.
    private func setContentSelection(_ selection: ContentSelection?, announce: Bool = true) {
        contentEdit.multiSelection = selection
        guard let selection else { return }
        contentEdit.selectedImage = nil
        contentEdit.selectedPath = nil
        if announce { editStatus = ContentEditing.selected(selection.count) }
    }

    /// What SHIFT+click extends: the multi-selection on this page, or else
    /// the single item already selected there.
    private func selectionToExtend(page: Int) -> ContentSelection {
        if let existing = contentEdit.multiSelection, existing.page == page { return existing }
        var start = ContentSelection(page: page)
        if let image = contentEdit.selectedImage, image.page == page { start = start.toggled(image) }
        if let path = contentEdit.selectedPath, path.page == page { start = start.toggled(path) }
        return start
    }
}
