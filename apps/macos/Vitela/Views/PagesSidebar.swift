import SwiftUI

/// The Pages column: every page as a small picture and its number, the
/// current one marked, a click going to it — Linux's numbered page buttons
/// and Windows' `PageNavigationList`, with the thumbnails Organize already
/// renders (`ViewerStore+Thumbnails.swift`), so the two share one cache.
struct PagesSidebar: View {
    @ObservedObject var model: ViewerViewModel
    @ObservedObject var store: ViewerStore
    @Environment(\.displayScale) private var displayScale

    /// The picture's box: narrower than an Organize card, from the same render.
    static let pictureBox = 96.0

    init(model: ViewerViewModel) {
        self.model = model
        store = model.store
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Pages").font(.headline)
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 10) {
                        ForEach(store.pageSlots, id: \.index) { slot in
                            row(slot)
                                .id(rowId(slot.index))
                                .onAppear { model.renderThumbnail(page: slot.index, scale: Double(displayScale)) }
                        }
                    }
                    .padding(.vertical, 4)
                }
                // Follows the reader, as Windows' list does, without
                // animating every page boundary the scroll crosses.
                .onChange(of: store.currentPage) { page in proxy.scrollTo(rowId(page)) }
            }
        }
        .padding(8)
        .disabled(!model.canPickPageFromList)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Pages")
    }

    private func row(_ slot: PageSlot) -> some View {
        let current = slot.index == store.currentPage
        return Button { model.pickPageFromList(slot.index) } label: {
            VStack(spacing: 4) {
                picture(store.thumbnails[slot.index])
                    .frame(width: Self.pictureBox, height: Self.pictureBox)
                Text("\(slot.index + 1)")
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(current ? .primary : .secondary)
            }
            .padding(6)
            .background(
                RoundedRectangle(cornerRadius: 6)
                    .fill(current ? Color.accentColor.opacity(0.2) : Color.clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Go to page \(slot.index + 1)")
        .accessibilityLabel("Go to page \(slot.index + 1)")
        .accessibilityAddTraits(current ? .isSelected : [])
    }

    @ViewBuilder private func picture(_ thumbnail: RenderedPage?) -> some View {
        if let image = thumbnail?.nsImage {
            Image(nsImage: image)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .background(Color.white)
                .shadow(radius: 1)
        } else {
            // A failed render stays here, like an Organize card: the number
            // below still says which page it is.
            RoundedRectangle(cornerRadius: 2).fill(Color.secondary.opacity(0.1))
        }
    }

    /// Generation-keyed like the page list, so a newly opened document's
    /// rows are not taken for the previous one's.
    private func rowId(_ page: Int) -> String { "pages-\(store.generation)-\(page)" }
}
