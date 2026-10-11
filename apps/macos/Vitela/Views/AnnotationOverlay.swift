import SwiftUI

/// Paints this session's annotations on one page, and the one being traced.
/// The core renders only what the file already carried; what the session
/// added is drawn here until a save bakes it in — the same split as Windows'
/// `RedrawAnnotations` and the Linux overlay. Positions come from the core's
/// turn-aware placement, so a rotated page gets them in the right place.
struct AnnotationOverlay: View {
    @ObservedObject var store: ViewerStore
    let page: Int
    let zoom: Double

    /// Windows' tracing color.
    private static let draftColor = Color(red: 218 / 255, green: 165 / 255, blue: 32 / 255)

    var body: some View {
        Canvas { context, _ in
            guard let placement = store.placement(ofPage: page, zoom: zoom) else { return }
            let gesture = store.annotationGesture
            for annotation in store.annotations(onPage: page) where annotation.id != gesture?.annotation.id {
                draw(annotation, placement: placement, selected: annotation.id == store.selectedAnnotationId, in: &context)
            }
            if let gesture, gesture.annotation.page == page {
                // The annotation under the pointer, where the drag would leave it.
                draw(moved(gesture), placement: placement, selected: true, in: &context)
            } else if let selected = store.selectedAnnotation, selected.page == page, let bounds = selected.bounds {
                drawHandles(bounds, placement: placement, in: &context)
            }
            if let draft = store.annotationDraft, draft.page == page, draft.tool == .ink {
                context.stroke(stroke(draft.points, placement: placement), with: .color(Self.draftColor), style: Self.inkStyle(selected: false))
            } else if let draft = store.annotationDraft, draft.page == page {
                let rect = ViewerStore.tracedRect(from: draft.origin, to: draft.current, tool: draft.tool)
                let box = frame(rect, placement: placement)
                if draft.tool == .underline || draft.tool == .strikeout {
                    context.fill(Path(box), with: .color(Self.draftColor))
                } else {
                    context.stroke(Path(box), with: .color(Self.draftColor), lineWidth: 2)
                }
            }
        }
        .allowsHitTesting(false)
    }

    private func draw(_ annotation: Annotation, placement: PagePlacement, selected: Bool, in context: inout GraphicsContext) {
        let color = Self.color(annotation.color ?? .annotationDefault)
        let lineWidth: CGFloat = selected ? 3 : 2
        if annotation.kind == .ink {
            context.stroke(stroke(annotation.points, placement: placement), with: .color(color.opacity(220 / 255)), style: Self.inkStyle(selected: selected))
            return
        }
        guard let rect = annotation.rect else { return }
        let box = frame(rect, placement: placement)
        switch annotation.kind {
        case .highlight:
            context.fill(Path(box), with: .color(color.opacity(100 / 255)))
            context.stroke(Path(box), with: .color(color.opacity(220 / 255)), lineWidth: lineWidth)
        case .underline, .strikeout:
            context.fill(Path(box), with: .color(color.opacity(220 / 255)))
        case .textNote:
            // A note is its icon's box: filled lightly so it reads as a
            // marker, not as an outline around content.
            context.fill(Path(roundedRect: box, cornerRadius: 3), with: .color(color.opacity(160 / 255)))
            context.stroke(Path(roundedRect: box, cornerRadius: 3), with: .color(color.opacity(220 / 255)), lineWidth: 1)
        default:
            context.stroke(Path(box), with: .color(color.opacity(220 / 255)), lineWidth: lineWidth)
        }
    }

    /// The gesture's annotation at its preview position: moved as a whole,
    /// or with the grabbed corner following the pointer.
    private func moved(_ gesture: AnnotationGesture) -> Annotation {
        let source = gesture.annotation
        return Annotation(
            id: source.id, page: source.page, kind: source.kind,
            rect: gesture.previewRect,
            points: source.points.map { PdfPoint(x: $0.x + gesture.dx, y: $0.y + gesture.dy) },
            color: source.color, contents: source.contents
        )
    }

    /// Fixed-size squares at the selection's corners, sized in screen points
    /// rather than scaled with the page so they stay grabbable at any zoom —
    /// Linux's `draw_handles`, in Windows' handle blue.
    private func drawHandles(_ bounds: PdfRect, placement: PagePlacement, in context: inout GraphicsContext) {
        let reach = ViewerStore.handleReach
        for corner in AnnotationCorner.allCases {
            let at = store.client.placePoint(corner.point(of: bounds), on: placement)
            let square = CGRect(x: at.left - reach / 2, y: at.top - reach / 2, width: reach, height: reach)
            context.fill(Path(square), with: .color(Self.handleColor))
        }
    }

    private static let handleColor = Color(red: 26 / 255, green: 89 / 255, blue: 217 / 255)

    /// A freehand stroke as a polyline through the placed points.
    private func stroke(_ points: [PdfPoint], placement: PagePlacement) -> Path {
        var path = Path()
        for (index, point) in points.enumerated() {
            let placed = store.client.placePoint(point, on: placement)
            let at = CGPoint(x: placed.left, y: placed.top)
            if index == 0 { path.move(to: at) } else { path.addLine(to: at) }
        }
        return path
    }

    private static func inkStyle(selected: Bool) -> StrokeStyle {
        StrokeStyle(lineWidth: selected ? 3 : 2, lineCap: .round, lineJoin: .round)
    }

    /// The placed rect, never thinner than 2 points on either axis — which
    /// screen axis a rule's thin side is on depends on the page's turn.
    private func frame(_ rect: PdfRect, placement: PagePlacement) -> CGRect {
        let placed = store.client.placeRect(rect, on: placement)
        return CGRect(x: placed.left, y: placed.top, width: max(2, placed.width), height: max(2, placed.height))
    }

    private static func color(_ color: PdfColor) -> Color {
        Color(red: Double(color.r) / 255, green: Double(color.g) / 255, blue: Double(color.b) / 255)
    }
}
