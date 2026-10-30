import SwiftUI

/// In Edit Text mode, the outline of every line, image and vector path on
/// one page that can be edited — the only thing the shell draws for
/// content, as on Android and Windows: the words themselves are the
/// renderer's. Placed with the core's turn-aware placement, like the
/// annotation overlay.
struct ContentOutlineOverlay: View {
    @ObservedObject var store: ViewerStore
    let page: Int
    let zoom: Double
    /// Asks for this page's content; a no-op once it is known or on its way.
    let read: () -> Void

    private static let outline = Color.accentColor.opacity(0.7)
    private static let imageOutline = Color.orange.opacity(0.8)
    /// Windows' `PathOutlineBrush`: faint, so a table's frame does not hide its text.
    private static let pathOutline = Color(red: 30 / 255, green: 150 / 255, blue: 90 / 255).opacity(0.45)

    var body: some View {
        Canvas { context, _ in
            guard store.contentEditMode,
                  let content = store.pageContent[page],
                  let placement = store.placement(ofPage: page, zoom: zoom)
            else { return }
            for image in content.images {
                context.stroke(Path(frame(image.bbox, placement)), with: .color(Self.imageOutline), style: StrokeStyle(lineWidth: 1, dash: [5, 3]))
            }
            for path in content.paths where path != store.selectedContentPath {
                context.stroke(Path(frame(path.bbox, placement)), with: .color(Self.pathOutline), style: StrokeStyle(lineWidth: 1, dash: [1, 2]))
            }
            if let selected = store.selectedContentImage, selected.page == page {
                selection(store.imageDrag.map(\.previewRect) ?? selected.bbox, color: Self.imageOutline, placement, in: &context)
            }
            if let selected = store.selectedContentPath, selected.page == page {
                selection(store.pathDrag.map(\.previewRect) ?? selected.bbox, color: .accentColor, placement, in: &context)
            }
            // One outline per block — the unit a click opens — as on Windows.
            for bbox in content.textBlocks.isEmpty ? content.textRuns.map(\.bbox) : content.textBlocks.map(\.bbox) {
                context.stroke(Path(frame(bbox, placement)), with: .color(Self.outline), style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
            }
            selectionOverlay(placement, in: &context)
            // Where a dragged line would land.
            if let drag = store.contentDrag, drag.moving, drag.run.page == page {
                context.stroke(Path(frame(drag.previewRect, placement)), with: .color(.accentColor), lineWidth: 2)
            }
        }
        .allowsHitTesting(false)
        .onAppear(perform: readIfEditing)
        .onChange(of: store.contentEditMode) { _ in readIfEditing() }
        // An edit forgets every page's content; read this one again.
        .onChange(of: store.pageContent[page] == nil) { _ in readIfEditing() }
    }

    /// A marquee being dragged, and a multi-selection: its members and the
    /// dashed box around them, shifted while it is dragged (B25 T-208).
    private func selectionOverlay(_ placement: PagePlacement, in context: inout GraphicsContext) {
        if let gesture = store.selectionGesture, gesture.page == page, gesture.kind == .marquee, gesture.dragging {
            let rect = Path(frame(gesture.rect, placement))
            context.fill(rect, with: .color(Color.accentColor.opacity(0.11)))
            context.stroke(rect, with: .color(.accentColor), style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
        }
        guard let selection = store.contentSelection, selection.page == page, let bbox = selection.bbox else { return }
        var dx = 0.0, dy = 0.0
        if let gesture = store.selectionGesture, gesture.kind == .drag, gesture.dragging {
            dx = gesture.current.x - gesture.origin.x
            dy = gesture.current.y - gesture.origin.y
        }
        func shifted(_ r: PdfRect) -> PdfRect { PdfRect(x: r.x + dx, y: r.y + dy, width: r.width, height: r.height) }
        for member in selection.blocks.map(\.bbox) + selection.images.map(\.bbox) + selection.paths.map(\.bbox) {
            context.stroke(Path(frame(shifted(member), placement)), with: .color(.accentColor), lineWidth: 1.5)
        }
        context.stroke(Path(frame(shifted(bbox), placement).insetBy(dx: -3, dy: -3)), with: .color(.accentColor),
                       style: StrokeStyle(lineWidth: 1, dash: [5, 3]))
    }

    /// The selected image's or path's frame and its four corner handles.
    private func selection(_ rect: PdfRect, color: Color, _ placement: PagePlacement, in context: inout GraphicsContext) {
        context.stroke(Path(frame(rect, placement)), with: .color(color), lineWidth: 2)
        for corner in AnnotationCorner.allCases {
            let point = store.client.placePoint(corner.point(of: rect), on: placement)
            let handle = CGRect(x: point.left - 4, y: point.top - 4, width: 8, height: 8)
            context.fill(Path(handle), with: .color(.white))
            context.stroke(Path(handle), with: .color(color), lineWidth: 1)
        }
    }

    private func frame(_ rect: PdfRect, _ placement: PagePlacement) -> CGRect {
        let placed = store.client.placeRect(rect, on: placement)
        return CGRect(x: placed.left, y: placed.top, width: placed.width, height: placed.height).insetBy(dx: -1, dy: -1)
    }

    private func readIfEditing() {
        if store.contentEditMode, store.pageContent[page] == nil { read() }
    }
}
