import SwiftUI

/// Shows a note's text without recording an edit — Windows' Read note.
struct ReadNoteSheet: View {
    let text: String
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Note").font(.headline)
            ScrollView {
                Text(text)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(minWidth: 300, minHeight: 120, maxHeight: 300)
            HStack {
                Spacer()
                Button("Close", action: close)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}
