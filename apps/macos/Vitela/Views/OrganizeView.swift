import SwiftUI
import UniformTypeIdentifiers

/// The Organize grid: every page as a card, standing in for the reader —
/// the macOS twin of Windows' `MainWindow.Organize.cs` and the Linux
/// `organize/grid`. Each action on a card — turn, delete, move, insert a
/// blank page before it, or drag it onto another card — and each blank page
/// or batch of PDFs added from the header is one undoable page edit.
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
                            enabled: model.store.canEditPages && !model.isImporting,
                            canDelete: model.store.pageSlots.count > 1,
                            isLast: slot.index == model.store.pageSlots.count - 1,
                            onEdit: { model.editPages($0(slot.index)) }
                        )
                        .onDrag {
                            guard model.store.canEditPages else { return NSItemProvider() }
                            return NSItemProvider(object: model.beginPageDrag(page: slot.index) as NSString)
                        }
                        .onDrop(of: [.plainText], delegate: PageDropTarget(model: model, target: slot.index))
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
            Button("Add PDFs…", action: model.importPdfs)
                .help("Add every page of other PDFs at the end")
                .disabled(!model.store.canEditPages || model.isImporting)
            Button("Add blank A4 page") { appendBlankPage(.portrait) }
                .disabled(!model.store.canEditPages)
            Button("Add landscape A4 page") { appendBlankPage(.landscape) }
                .disabled(!model.store.canEditPages)
            if model.store.pageWorkInFlight || model.isImporting {
                ProgressView().controlSize(.small)
            }
            Button("Done") { model.isOrganizing = false }
                .keyboardShortcut(.cancelAction)
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }
}

/// Accepts a dragged Organize card on another card. The move itself is the
/// view model's: this only reads the drag's token back out of the payload.
private struct PageDropTarget: DropDelegate {
    let model: ViewerViewModel
    let target: Int

    func validateDrop(info: DropInfo) -> Bool {
        model.organizePageDrag != nil && model.store.canEditPages && info.hasItemsConforming(to: [.plainText])
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: validateDrop(info: info) ? .move : .forbidden)
    }

    func performDrop(info: DropInfo) -> Bool {
        guard validateDrop(info: info), let provider = info.itemProviders(for: [.plainText]).first else { return false }
        _ = provider.loadObject(ofClass: NSString.self) { token, _ in
            guard let token = token as? String else { return }
            DispatchQueue.main.async { model.dropPage(token: token, on: target) }
        }
        return true
    }
}
