import SwiftUI

/// Asks for pages as typed text — Windows' "Extract pages" and "Split PDF"
/// dialogs, which differ only in their words. Stays open on a refusal,
/// showing the core's own sentence, so the reader corrects it in place.
struct PageRangeSheet: View {
    let title: String
    let placeholder: String
    let hint: String
    let actionTitle: String
    /// Returns the refusal to show, or `nil` once the sheet can close.
    let submit: (String) -> String?
    let cancel: () -> Void

    @State private var text = ""
    @State private var refusal: String?
    @FocusState private var fieldFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(title).font(.headline)
            TextField(placeholder, text: $text)
                .textFieldStyle(.roundedBorder)
                .focused($fieldFocused)
                .onSubmit(run)
                .accessibilityIdentifier("page-range")
            Text(hint)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let refusal {
                Text(refusal)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("page-range-refusal")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: cancel)
                    .keyboardShortcut(.cancelAction)
                Button(actionTitle, action: run)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 360)
        .onAppear { fieldFocused = true }
    }

    private func run() {
        refusal = submit(text)
    }
}

extension PageRangeSheet {
    static func extract(_ model: ViewerViewModel) -> PageRangeSheet {
        let count = model.store.pageSlots.count
        return PageRangeSheet(
            title: "Extract pages",
            placeholder: "Pages to extract, e.g. 1-3,7",
            hint: "This document has \(count) \(count == 1 ? "page" : "pages").",
            actionTitle: "Extract",
            submit: model.extractPages(range:),
            cancel: model.cancelExtractPages
        )
    }

    static func split(_ model: ViewerViewModel) -> PageRangeSheet {
        PageRangeSheet(
            title: "Split PDF",
            placeholder: "Split after page, e.g. 3,7",
            hint: "This document has \(model.store.pageSlots.count) pages. Each cut starts a new file.",
            actionTitle: "Split",
            submit: model.splitPages(cuts:),
            cancel: model.cancelSplitPages
        )
    }
}
