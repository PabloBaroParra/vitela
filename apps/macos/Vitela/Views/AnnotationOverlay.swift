import AppKit
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
        if annotation.kind == .freeText {
            return drawFreeText(annotation, at: rect, placement: placement, selected: selected, in: &context)
        }
        if annotation.kind == .stamp, let image = StampPreviews.image(for: annotation.id, bytes: store.stampImages[annotation.id]) {
            // Sized upright and turned with the page: a saved stamp's
            // appearance turns with the page, so its preview does too.
            let upright = uprightContext(rect, placement: placement, from: context)
            upright.draw(Image(nsImage: image), in: CGRect(x: 0, y: 0, width: rect.width * placement.scale, height: rect.height * placement.scale))
            if selected {
                context.stroke(Path(frame(rect, placement: placement)), with: .color(Self.handleColor), lineWidth: 1)
            }
            return
        }
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

    /// A text box: the core's lines, laid out upright in the box's own frame
    /// and turned with the page about the box's upright top-left — Windows'
    /// `DrawFreeText` with `PlaceUpright`. Never a wrap computed here.
    private func drawFreeText(_ annotation: Annotation, at rect: PdfRect, placement: PagePlacement, selected: Bool, in context: inout GraphicsContext) {
        if selected {
            // The box has no border in the file; this outline only shows its edge.
            let box = frame(rect, placement: placement)
            context.stroke(Path(box), with: .color(Self.handleColor), style: StrokeStyle(lineWidth: 1, dash: [4, 3]))
        }
        guard let layout = store.freeTextLayout(for: annotation, at: rect) else { return }
        let scale = placement.scale
        let upright = uprightContext(rect, placement: placement, from: context)
        // The core hands over baselines; text is placed by its top, so step
        // up by the face's ascent — what Windows' `FreeTextDraw.AscentEm` does.
        let size = layout.fontSizePt * scale
        let face = NSFont(name: "Helvetica", size: size) ?? NSFont.systemFont(ofSize: size)
        let font = Font(face as CTFont)
        for line in layout.lines {
            upright.draw(
                Text(line.text).font(font).foregroundColor(.black),
                at: CGPoint(x: line.xPt * scale, y: line.baselineFromTopPt * scale - face.ascender),
                anchor: .topLeading
            )
        }
    }

    /// A context whose origin is `rect`'s upright top-left on the drawn page,
    /// turned with the page and clipped to the rect — Windows' `PlaceUpright`.
    private func uprightContext(_ rect: PdfRect, placement: PagePlacement, from context: GraphicsContext) -> GraphicsContext {
        let anchor = store.client.placePoint(PdfPoint(x: rect.x, y: rect.y + rect.height), on: placement)
        var upright = context
        upright.translateBy(x: anchor.left, y: anchor.top)
        upright.rotate(by: .degrees(Self.degrees(placement.rotation)))
        upright.clip(to: Path(CGRect(x: 0, y: 0, width: rect.width * placement.scale, height: rect.height * placement.scale)))
        return upright
    }

    private static func degrees(_ rotation: PageRotation) -> Double {
        switch rotation {
        case .none: return 0
        case .clockwise90: return 90
        case .clockwise180: return 180
        case .clockwise270: return 270
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
            color: source.color, contents: source.contents, freeTextLayout: source.freeTextLayout
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

/// Decoded stamp previews, so a redraw does not decode the image again.
/// Keyed by the bytes as well as the id: annotation ids restart with each
/// document, the bytes do not repeat by accident.
private enum StampPreviews {
    private static let cache = NSCache<NSString, NSImage>()

    static func image(for id: UInt64, bytes: Data?) -> NSImage? {
        guard let bytes else { return nil }
        let key = "\(id)-\(bytes.count)-\(bytes.hashValue)" as NSString
        if let cached = cache.object(forKey: key) { return cached }
        guard let image = NSImage(data: bytes) else { return nil }
        cache.setObject(image, forKey: key)
        return image
    }
}
