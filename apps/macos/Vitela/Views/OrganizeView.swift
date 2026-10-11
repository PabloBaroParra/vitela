import SwiftUI

/// The Organize grid: every page as a card, standing in for the reader —
/// the macOS twin of Windows' `MainWindow.Organize.cs` and the Linux
/// `organize/grid`. Each action on a card — turn, delete, insert a blank
/// page before it — and each blank page added from the header is one
/// undoable page edit.
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
                            canDelete: model.store.pageSlots.count > 1,
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

    private func appendBlankPage(_ orientation: PageEdit.Orientation) {
        model.editPages(.insertBlank(at: model.store.pageSlots.count, orientation: orientation))
    }

    private var header: some View {
        HStack {
            Text("Organize pages").font(.headline)
            Text("Each change is one undo step.").foregroundStyle(.secondary)
            Spacer()
            Button("Add blank A4 page") { appendBlankPage(.portrait) }
                .disabled(!model.store.canEditPages)
            Button("Add landscape A4 page") { appendBlankPage(.landscape) }
                .disabled(!model.store.canEditPages)
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
