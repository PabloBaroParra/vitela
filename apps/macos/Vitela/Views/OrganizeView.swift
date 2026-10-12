import SwiftUI

/// The Organize grid: every page as a card, standing in for the reader —
/// the macOS twin of Windows' `MainWindow.Organize.cs` and the Linux
/// `organize/grid`. Each action on a card is one undoable page edit.
struct OrganizeView: View {
    @ObservedObject var model: ViewerViewModel
    @Environment(\.displayScale) private var displayScale

    private let columns = [GridItem(.adaptive(minimum: ViewerStore.thumbnailBox + 24), spacing: 16)]

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
            ScrollView {
                LazyVGrid(columns: columns, spacing: 16) {
                    ForEach(model.store.pageSlots, id: \.index) { slot in
                        OrganizePageCard(
                            number: slot.index + 1,
                            thumbnail: model.store.thumbnails[slot.index],
                            enabled: model.store.canEditPages,
                            onEdit: { model.editPages($0(slot.index)) }
                        )
                        .onAppear { model.renderThumbnail(page: slot.index, scale: Double(displayScale)) }
                        // Same reason as the reader's rows: a relayout reuses
                        // page indices, and only a new identity re-fires
                        // `onAppear` to ask for the new picture.
                        .id("\(model.store.generation)-\(slot.index)")
                    }
                }
                .padding()
            }
        }
        .accessibilityIdentifier("organize-grid")
    }

    private var header: some View {
        HStack {
            Text("Organize pages").font(.headline)
            Text("Each change is one undo step.").foregroundStyle(.secondary)
            Spacer()
            if model.store.pageWorkInFlight {
                ProgressView().controlSize(.small)
            }
            Button("Done") { model.isOrganizing = false }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}
