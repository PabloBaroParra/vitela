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

    var body: some View {
        Canvas { context, _ in
            guard store.contentEditMode,
                  let runs = store.pageContent[page]?.textRuns,
                  let placement = store.placement(ofPage: page, zoom: zoom)
            else { return }
            for run in runs {
                let placed = store.client.placeRect(run.bbox, on: placement)
                let frame = CGRect(x: placed.left, y: placed.top, width: placed.width, height: placed.height).insetBy(dx: -1, dy: -1)
                context.stroke(Path(frame), with: .color(Self.outline), style: StrokeStyle(lineWidth: 1, dash: [3, 2]))
            }
        }
        .allowsHitTesting(false)
        .onAppear(perform: readIfEditing)
        .onChange(of: store.contentEditMode) { _ in readIfEditing() }
        // An edit forgets every page's content; read this one again.
        .onChange(of: store.pageContent[page] == nil) { _ in readIfEditing() }
    }

    private func readIfEditing() {
        if store.contentEditMode, store.pageContent[page] == nil { read() }
    }
}
