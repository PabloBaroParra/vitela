import SwiftUI

/// Asks for a page number. A sheet rather than an alert with a text field:
/// alert text fields need macOS 13, and the deployment floor is 12.
struct GoToPageSheet: View {
    @ObservedObject var model: ViewerViewModel
    @State private var text = ""
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Go to page").font(.headline)
            TextField("Page number (1–\(model.store.pageCount))", text: $text)
                .textFieldStyle(.roundedBorder)
                .focused($fieldFocused)
                .onSubmit { model.goToTypedPage(text) }
                .accessibilityIdentifier("go-to-page-number")
            HStack {
                Spacer()
                Button("Cancel", role: .cancel) { model.isGoToPagePresented = false }
                    .keyboardShortcut(.cancelAction)
                Button("Go") { model.goToTypedPage(text) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(model.store.pageIndex(forTypedNumber: text) == nil)
            }
        }
        .padding()
        .frame(width: 280)
        .onAppear {
            text = String(model.store.currentPage + 1)
            fieldFocused = true
        }
    }
}
