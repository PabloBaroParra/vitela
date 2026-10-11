import SwiftUI

/// Asks for a just-placed note's text — Windows' "Add note — page N" dialog.
/// Add stays disabled until there is something to say.
struct NoteSheet: View {
    @ObservedObject var store: ViewerStore
    @State private var text = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Add note — page \((store.pendingNote?.page ?? 0) + 1)").font(.headline)
            TextEditor(text: $text)
                .font(.body)
                .frame(minWidth: 300, minHeight: 120, maxHeight: 300)
                .border(Color.secondary.opacity(0.4))
                .accessibilityIdentifier("note-text")
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: store.cancelPendingNote)
                    .keyboardShortcut(.cancelAction)
                Button("Add") { store.commitPendingNote(text: text) }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding()
    }
}
