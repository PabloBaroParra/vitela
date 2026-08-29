import SwiftUI

/// Paints search-match and text-selection highlights over one page, and
/// turns a touch drag on that page into selection calls. Kept apart from
/// `PageView` — which only knows how to paint the page bitmap — so drag and
/// highlight logic don't dilute that view's one job. Mirrors macOS's
/// `SelectionOverlay`; the gesture is the one real difference between the
/// two, noted below.
struct SelectionOverlay: View {
    let dimensions: PageDimensions
    let zoom: Double
    let selectionRects: [TextRect]
    let matchRects: [TextRect]
    let currentMatchRects: [TextRect]
    let onDragBegan: (CGPoint) -> Void
    let onDragChanged: (CGPoint) -> Void

    /// Distinguishes the drag's first sample (a fresh press, which should
    /// start a new selection) from a later one (which should extend it) —
    /// same role as macOS's `isDragging`.
    @State private var isSelecting = false

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
            // A plain `DragGesture` here would compete with the page list's
            // `ScrollView` for every single-finger touch, so a swipe to
            // scroll would never win. Requiring a long press first — the
            // same gesture UIKit's own text selection uses — lets a quick
            // swipe pass through to the scroll view untouched, and only a
            // deliberate press-and-hold starts a selection drag.
            LongPressGesture(minimumDuration: 0.35)
                .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .local))
                .onChanged { value in
                    guard case let .second(true, drag?) = value else { return }
                    if isSelecting {
                        onDragChanged(drag.location)
                    } else {
                        isSelecting = true
                        onDragBegan(drag.location)
                    }
                }
                .onEnded { _ in isSelecting = false }
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
