import SwiftUI

/// The search and page-navigation groups in Linux's FlowBox and Windows'
/// ToolbarPanel. They share a row when there is room, never compressing the
/// page position to an unreadable fragment. Search feedback gets its own
/// wrapping line so a long query cannot push navigation outside the window.
struct SearchNavigationBar: View {
    @ObservedObject var model: ViewerViewModel
    @Binding var query: String
    @FocusState.Binding var searchFieldFocused: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 320), alignment: .leading)], alignment: .leading, spacing: 8) {
                searchControls
                PageNavigationBar(model: model)
                    .fixedSize(horizontal: true, vertical: false)
            }
            if !model.store.searchStatus.isEmpty {
                Text(model.store.searchStatus)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
    }

    private var searchControls: some View {
        HStack {
            TextField("Find in document", text: $query)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 160, maxWidth: 240)
                .focused($searchFieldFocused)
                .onSubmit { model.runSearch(query) }
            Button { model.runSearch(query) } label: { Label("Find in document", shellIcon: .search) }
                .labelStyle(.iconOnly)
                .help("Find in document (⌘F)")
            Button { model.stepSearchMatch(by: -1) } label: { Label("Previous match", shellIcon: .previous) }
                .labelStyle(.iconOnly)
                .help("Previous match")
                .disabled(model.store.search == nil)
            Button { model.stepSearchMatch(by: 1) } label: { Label("Next match", shellIcon: .next) }
                .labelStyle(.iconOnly)
                .help("Next match")
                .disabled(model.store.search == nil)
            Spacer(minLength: 0)
        }
    }
}
