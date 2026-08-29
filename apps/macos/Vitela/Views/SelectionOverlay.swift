import SwiftUI

/// Paints search-match and text-selection highlights over one page, and
/// turns a pointer drag on that page into selection calls. Kept apart from
/// `PageView` — which only knows how to paint the page bitmap — so drag and
/// highlight logic don't dilute that view's one job.
struct SelectionOverlay: View {
    let dimensions: PageDimensions
    let zoom: Double
    let selectionRects: [TextRect]
    let matchRects: [TextRect]
    let currentMatchRects: [TextRect]
    let onDragBegan: (CGPoint) -> Void
    let onDragChanged: (CGPoint) -> Void

    /// Distinguishes the drag's first sample (a fresh mouse-down, which
    /// should start a new selection) from a later one (which should extend
    /// it). `DragGesture(minimumDistance: 0)` fires `onChanged` for both, so
    /// this state — not the gesture's own `translation` — is what tells them
    /// apart reliably, including a plain click that never moves at all.
    @State private var isDragging = false

    var body: some View {
        Canvas { context, _ in
            for rect in matchRects {
                context.fill(Path(viewRect(rect)), with: .color(.yellow.opacity(0.4)))
            }
            for rect in currentMatchRects {
                context.fill(Path(viewRect(rect)), with: .color(.orange.opacity(0.6)))
            }
            for rect in selectionRects {
                context.fill(Path(viewRect(rect)), with: .color(.accentColor.opacity(0.35)))
            }
        }
        .contentShape(Rectangle())
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .local)
                .onChanged { value in
                    if isDragging {
                        onDragChanged(value.location)
                    } else {
                        isDragging = true
                        onDragBegan(value.location)
                    }
                }
                .onEnded { _ in isDragging = false }
        )
    }

    private func viewRect(_ rect: TextRect) -> CGRect {
        CGRect(
            x: rect.xPt * zoom,
            y: (dimensions.height - rect.yPt - rect.heightPt) * zoom,
            width: rect.widthPt * zoom,
            height: rect.heightPt * zoom
        )
    }
}
