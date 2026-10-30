import SwiftUI

/// Retypes, moves to typed coordinates, or deletes one text block the page
/// paints — a run, or the runs the core reads as one piece of text (B25
/// T-207) — Android's retype dialog (`ContentEditDialog.kt`) plus Windows'
/// Move text; Windows types into the page itself. Each is one undoable edit;
/// a composite-font block says up front that its font will be replaced.
struct ContentTextSheet: View {
    @ObservedObject var model: ViewerViewModel
    let block: ContentTextBlock
    @State private var text: String
    @State private var x: Double
    @State private var y: Double
    @State private var moveProblem: String?

    init(model: ViewerViewModel, block: ContentTextBlock) {
        self.model = model
        self.block = block
        _text = State(initialValue: block.text)
        _x = State(initialValue: block.bbox.x)
        _y = State(initialValue: block.bbox.y)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Edit text on page \(block.page + 1)").font(.headline)
            TextField("Text", text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 320)
                .onSubmit(save)
                .accessibilityIdentifier("content-text")
            if block.substitutesFont {
                Text(ContentEditing.fontSubstituted)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Divider()
            // Windows' Move text: where the line's box starts, in points.
            HStack(spacing: 8) {
                Text("Position")
                coordinate("X", $x)
                coordinate("Y", $y)
                Button("Move") { moveProblem = model.moveText(block, toX: x, y: y) }
            }
            Text(ViewerStore.ContentGeometry.textHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let moveProblem {
                Text(moveProblem).foregroundColor(.red).fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Button("Delete Text", role: .destructive) { model.deleteText(block) }
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelTextEdit)
                    .keyboardShortcut(.cancelAction)
                Button("Save", action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || text == block.text)
            }
        }
        .padding()
    }

    private func coordinate(_ label: String, _ value: Binding<Double>) -> some View {
        HStack(spacing: 4) {
            Text(label).foregroundStyle(.secondary)
            TextField(label, value: value, formatter: GeometrySheet.points)
                .textFieldStyle(.roundedBorder)
                .frame(width: 72)
                .accessibilityIdentifier("content-text-\(label.lowercased())")
        }
    }

    private func save() {
        model.retypeText(block, to: text)
    }
}
