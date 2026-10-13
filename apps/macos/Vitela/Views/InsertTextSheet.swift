import SwiftUI

/// The new line Add Text places — Android's insert dialog: one line of
/// Helvetica and its size, 14 pt to start. Stays open with what was typed
/// when the line or the size will not do.
struct InsertTextSheet: View {
    @ObservedObject var model: ViewerViewModel
    let page: Int
    @State private var text = ""
    @State private var size = String(Int(ContentEditing.defaultInsertedTextSize))
    @State private var problem: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Add text to page \(page + 1)").font(.headline)
            TextField("Text", text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 320)
                .onSubmit(insert)
                .accessibilityIdentifier("insert-text")
            HStack {
                Text("Size")
                TextField("pt", text: $size)
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 56)
                Text("pt").foregroundStyle(.secondary)
            }
            if let problem {
                Text(problem).foregroundColor(.red).fixedSize(horizontal: false, vertical: true)
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelTextInsert)
                    .keyboardShortcut(.cancelAction)
                Button("Add", action: insert)
                    .keyboardShortcut(.defaultAction)
                    .disabled(!ContentEditing.insertableText(text))
            }
        }
        .padding()
        .onChange(of: text) { _ in problem = nil }
        .onChange(of: size) { _ in problem = nil }
    }

    private func insert() {
        problem = model.insertText(text, size: size)
    }
}
