import SwiftUI

/// Asks for a text box's text — Windows' "Add text box" / "Edit text box"
/// dialog. Stays open on the core's refusal with the text as typed, so a
/// character the font cannot show can be replaced in place.
struct FreeTextSheet: View {
    @ObservedObject var store: ViewerStore
    let pending: PendingFreeText
    @State private var text: String
    @State private var problem: String?

    init(store: ViewerStore, pending: PendingFreeText) {
        self.store = store
        self.pending = pending
        _text = State(initialValue: pending.initialText)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(pending.title).font(.headline)
            TextEditor(text: $text)
                .font(.body)
                .frame(minWidth: 300, minHeight: 120, maxHeight: 300)
                .border(Color.secondary.opacity(0.4))
                .onChange(of: text) { _ in problem = nil }
                .accessibilityIdentifier("free-text")
            if let problem {
                Text(problem)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("free-text-problem")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: store.cancelPendingFreeText)
                    .keyboardShortcut(.cancelAction)
                Button(pending.actionTitle) { problem = store.commitPendingFreeText(text) }
                    .disabled(!ViewerStore.freeTextAddEnabled(text))
            }
        }
        .padding()
    }
}
