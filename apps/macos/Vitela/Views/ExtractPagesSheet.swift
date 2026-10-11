import SwiftUI

/// Asks which pages to extract — Windows' "Extract pages" dialog. Stays open
/// on a refused range, showing the core's own sentence, so the reader
/// corrects it in place.
struct ExtractPagesSheet: View {
    @ObservedObject var model: ViewerViewModel
    @State private var range = ""
    @State private var refusal: String?
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Extract pages").font(.headline)
            TextField("Pages to extract, e.g. 1-3,7", text: $range)
                .textFieldStyle(.roundedBorder)
                .focused($fieldFocused)
                .onSubmit(extract)
                .accessibilityIdentifier("extract-page-range")
            Text("This document has \(model.store.pageSlots.count) \(model.store.pageSlots.count == 1 ? "page" : "pages").")
                .foregroundStyle(.secondary)
            if let refusal {
                Text(refusal)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("extract-refusal")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelExtractPages)
                    .keyboardShortcut(.cancelAction)
                Button("Extract", action: extract)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 360)
        .onAppear { fieldFocused = true }
    }

    private func extract() {
        refusal = model.extractPages(range: range)
    }
}
