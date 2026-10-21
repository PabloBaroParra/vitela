import SwiftUI

/// Two numbers in points — a position or a size — for whatever is selected:
/// Windows' Move and Resize dialogs, for an annotation
/// (`ViewerStore+AnnotationGeometry.swift`) or an image the page paints
/// (`ViewerStore+ContentGeometry.swift`). Starts from the current numbers and
/// stays open with the reason when they will not do.
struct GeometrySheet: View {
    /// What the sheet asks and does; `apply` returns why it must stay open.
    struct Request: Identifiable {
        let title: String
        let labels: (String, String)
        let values: (Double, Double)
        let hint: String
        let action: String
        let apply: (Double, Double) -> String?
        var id: String { title + action }
    }

    let request: Request
    let close: () -> Void
    @State private var first: Double
    @State private var second: Double
    @State private var refusal: String?

    init(request: Request, close: @escaping () -> Void) {
        self.request = request
        self.close = close
        _first = State(initialValue: request.values.0)
        _second = State(initialValue: request.values.1)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(request.title).font(.headline)
            HStack(spacing: 16) {
                field(request.labels.0, $first)
                field(request.labels.1, $second)
            }
            Text(request.hint)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let refusal {
                Text(refusal).foregroundColor(.red).fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: close)
                    .keyboardShortcut(.cancelAction)
                Button(request.action, action: apply)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 360)
    }

    private func field(_ label: String, _ value: Binding<Double>) -> some View {
        HStack(spacing: 4) {
            Text(label)
            TextField(label, value: value, formatter: Self.points)
                .textFieldStyle(.roundedBorder)
                .frame(width: 80)
                .accessibilityIdentifier("geometry-\(label.lowercased())")
            Text("pt").foregroundStyle(.secondary)
        }
    }

    static let points: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.maximumFractionDigits = 2
        return formatter
    }()

    private func apply() {
        refusal = request.apply(first, second)
        if refusal == nil { close() }
    }
}

extension GeometrySheet.Request {
    /// The selected annotation's Position… or Size….
    static func annotation(_ store: ViewerStore, position: Bool) -> Self? {
        guard let selected = store.selectedAnnotation, let box = position ? selected.bounds : selected.rect else { return nil }
        let page = selected.page + 1
        return position
            ? Self(title: "Move annotation — page \(page)", labels: ("X", "Y"), values: (box.x, box.y),
                   hint: ViewerStore.AnnotationGeometry.positionHint, action: "Move",
                   apply: { store.moveSelectedAnnotation(toX: $0, y: $1) })
            : Self(title: "Resize annotation — page \(page)", labels: ("Width", "Height"), values: (box.width, box.height),
                   hint: ViewerStore.AnnotationGeometry.sizeHint, action: "Resize",
                   apply: { store.resizeSelectedAnnotation(width: $0, height: $1) })
    }
}
