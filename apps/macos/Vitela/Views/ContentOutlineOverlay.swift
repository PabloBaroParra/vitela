import SwiftUI

/// In Edit Text mode, the outline of every line on one page that can be
/// retyped — the only thing the shell draws for content, as on Android and
/// Windows: the words themselves are the renderer's. Placed with the core's
/// turn-aware placement, like the annotation overlay.
struct ContentOutlineOverlay: View {
    @ObservedObject var store: ViewerStore
    let page: Int
    let zoom: Double
    /// Asks for this page's content; a no-op once it is known or on its way.
    let read: () -> Void

    private static let outline = Color.accentColor.opacity(0.7)
    private static let imageOutline = Color.orange.opacity(0.8)

    var body: some View {
        Canvas { context, _ in
            guard store.contentEditMode,
                  let content = store.pageContent[page],
                  let placement = store.placement(ofPage: page, zoom: zoom)
            else { return }
            for image in content.images {
                context.stroke(Path(frame(image.bbox, placement)), with: .color(Self.imageOutline), style: StrokeStyle(lineWidth: 1, dash: [5, 3]))
            }
            if let selected = store.selectedContentImage, selected.page == page {
                let rect = store.imageDrag.map(\.previewRect) ?? selected.bbox
                context.stroke(Path(frame(rect, placement)), with: .color(Self.imageOutline), lineWidth: 2)
                for corner in AnnotationCorner.allCases {
                    let point = store.client.placePoint(corner.point(of: rect), on: placement)
                    let handle = CGRect(x: point.left - 4, y: point.top - 4, width: 8, height: 8)
                    context.fill(Path(handle), with: .color(.white))
                    context.stroke(Path(handle), with: .color(Self.imageOutline), lineWidth: 1)
                }
            }
            for run in content.textRuns {
                context.stroke(Path(frame(run.bbox, placement)), with: .color(Self.outline), style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
            }
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

    private func frame(_ rect: PdfRect, _ placement: PagePlacement) -> CGRect {
        let placed = store.client.placeRect(rect, on: placement)
        return CGRect(x: placed.left, y: placed.top, width: placed.width, height: placed.height).insetBy(dx: -1, dy: -1)
    }

    private func readIfEditing() {
        if store.contentEditMode, store.pageContent[page] == nil { read() }
    }
}
