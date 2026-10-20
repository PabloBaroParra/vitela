import SwiftUI

/// Exact position or size for the selected annotation — Windows' Move and
/// Resize dialogs: two numbers in points, starting from where it is now.
/// Stays open with the reason when the numbers will not do.
struct AnnotationGeometrySheet: View {
    enum Mode: Identifiable {
        case position, size
        var id: Self { self }
    }

    @ObservedObject var store: ViewerStore
    let mode: Mode
    let close: () -> Void
    @State private var first: Double
    @State private var second: Double
    @State private var refusal: String?

    init(store: ViewerStore, mode: Mode, close: @escaping () -> Void) {
        self.store = store
        self.mode = mode
        self.close = close
        let selected = store.selectedAnnotation
        let box = mode == .position ? selected?.bounds : selected?.rect
        _first = State(initialValue: mode == .position ? box?.x ?? 0 : box?.width ?? 0)
        _second = State(initialValue: mode == .position ? box?.y ?? 0 : box?.height ?? 0)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline)
            HStack(spacing: 16) {
                field(mode == .position ? "X" : "Width", $first)
                field(mode == .position ? "Y" : "Height", $second)
            }
            Text(mode == .position ? ViewerStore.AnnotationGeometry.positionHint : ViewerStore.AnnotationGeometry.sizeHint)
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
                Button(mode == .position ? "Move" : "Resize", action: apply)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 360)
    }

    private var title: String {
        let page = (store.selectedAnnotation?.page ?? 0) + 1
        return mode == .position ? "Move annotation — page \(page)" : "Resize annotation — page \(page)"
    }

    private func field(_ label: String, _ value: Binding<Double>) -> some View {
        HStack(spacing: 4) {
            Text(label)
            TextField(label, value: value, formatter: Self.points)
                .textFieldStyle(.roundedBorder)
                .frame(width: 80)
                .accessibilityIdentifier("annotation-\(label.lowercased())")
            Text("pt").foregroundStyle(.secondary)
        }
    }

    private static let points: NumberFormatter = {
        let formatter = NumberFormatter()
        formatter.numberStyle = .decimal
        formatter.maximumFractionDigits = 2
        return formatter
    }()

    private func apply() {
        refusal = mode == .position
            ? store.moveSelectedAnnotation(toX: first, y: second)
            : store.resizeSelectedAnnotation(width: first, height: second)
        if refusal == nil { close() }
    }
}
