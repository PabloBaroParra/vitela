import SwiftUI

/// The page position readout and its controls — the macOS twin of the
/// Windows toolbar's First/Previous/Next/Last/Go to page buttons. Shortcuts
/// live on the Go menu (`VitelaCommands`), so they also work with the focus
/// in a text field.
struct PageNavigationBar: View {
    @ObservedObject var model: ViewerViewModel

    var body: some View {
        HStack(spacing: 4) {
            Button { model.store.goToFirstPage() } label: { ShellIconImage(icon: .firstPage) }
                .help("Go to the first page (⌘Home)")
                .accessibilityLabel("First page")
                .disabled(!model.store.canGoToPreviousPage)
            Button { model.store.stepPage(by: -1) } label: { ShellIconImage(icon: .previous) }
                .help("Go to the previous page (⌥⌘↑)")
                .accessibilityLabel("Previous page")
                .disabled(!model.store.canGoToPreviousPage)
            Button(model.store.pagePositionText ?? "No pages", action: model.requestGoToPage)
                .help("Jump to a page by number (⌥⌘G)")
                .accessibilityIdentifier("page-position")
                .disabled(model.store.pageCount == 0)
            Button { model.store.stepPage(by: 1) } label: { ShellIconImage(icon: .next) }
                .help("Go to the next page (⌥⌘↓)")
                .accessibilityLabel("Next page")
                .disabled(!model.store.canGoToNextPage)
            Button { model.store.goToLastPage() } label: { ShellIconImage(icon: .lastPage) }
                .help("Go to the last page (⌘End)")
                .accessibilityLabel("Last page")
                .disabled(!model.store.canGoToNextPage)
        }
    }
}
