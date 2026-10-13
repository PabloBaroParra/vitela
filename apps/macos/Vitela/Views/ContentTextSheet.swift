import SwiftUI

/// Retypes or deletes one line the page paints — Android's retype dialog
/// (`ContentEditDialog.kt`); Windows types into the page itself. Save is one
/// undoable edit; a composite-font line says up front that its font will be
/// replaced.
struct ContentTextSheet: View {
    @ObservedObject var model: ViewerViewModel
    let run: ContentTextRun
    @State private var text: String

    init(model: ViewerViewModel, run: ContentTextRun) {
        self.model = model
        self.run = run
        _text = State(initialValue: run.text)
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

    private func save() {
        model.retypeText(run, to: text)
    }
}
