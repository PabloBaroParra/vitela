import SwiftUI

/// Retypes, moves to typed coordinates, or deletes one line the page paints
/// — Android's retype dialog (`ContentEditDialog.kt`) plus Windows' Move
/// text; Windows types into the page itself. Each is one undoable edit; a
/// composite-font line says up front that its font will be replaced.
struct ContentTextSheet: View {
    @ObservedObject var model: ViewerViewModel
    let run: ContentTextRun
    @State private var text: String
    @State private var x: Double
    @State private var y: Double
    @State private var moveProblem: String?

    init(model: ViewerViewModel, run: ContentTextRun) {
        self.model = model
        self.run = run
        _text = State(initialValue: run.text)
        _x = State(initialValue: run.bbox.x)
        _y = State(initialValue: run.bbox.y)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Edit text on page \(run.page + 1)").font(.headline)
            TextField("Text", text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 320)
                .onSubmit(save)
                .accessibilityIdentifier("content-text")
            if run.substitutesFont {
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
                Button("Move") { moveProblem = model.moveText(run, toX: x, y: y) }
            }
            Text(ViewerStore.ContentGeometry.textHint)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let moveProblem {
                Text(moveProblem).foregroundColor(.red).fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Button("Delete Text", role: .destructive) { model.deleteText(run) }
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelTextEdit)
                    .keyboardShortcut(.cancelAction)
                Button("Save", action: save)
                    .keyboardShortcut(.defaultAction)
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || text == run.text)
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
        model.retypeText(run, to: text)
    }
}
