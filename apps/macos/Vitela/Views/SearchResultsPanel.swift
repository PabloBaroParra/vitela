import SwiftUI

/// Every match of the current search, in document order — Windows'
/// `SearchResultsList` in its Tools column. Clicking one makes it current,
/// which scrolls its page into view like Previous/Next; the list follows
/// Previous/Next in turn.
struct SearchResultsPanel: View {
    @ObservedObject var store: ViewerStore

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Results").font(.headline)
                Spacer()
                Button("Done", action: store.clearSearch)
                    .help("Clear the search")
            }
            Text(store.searchStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
            ScrollViewReader { proxy in
                List(store.searchResultRows, id: \.index) { row in
                    Button { store.selectSearchMatch(row.index) } label: {
                        Text(row.label)
                            .lineLimit(2)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .padding(.vertical, 2)
                    .listRowBackground(row.index == store.search?.currentIndex ? Color.vitelaAccent.opacity(0.2) : Color.clear)
                    .accessibilityAddTraits(row.index == store.search?.currentIndex ? .isSelected : [])
                    .id(row.index)
                }
                .onChange(of: store.search?.currentIndex) { index in
                    if let index { proxy.scrollTo(index) }
                }
            }
        }
        .padding(12)
    }
}
