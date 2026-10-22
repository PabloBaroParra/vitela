import SwiftUI
import UniformTypeIdentifiers

/// The Organize screen, standing in for the reader: the Documents view
/// (`OrganizeDocumentsList`) or the Pages grid, every page as a card — the
/// macOS twin of Windows' `MainWindow.Organize.cs` and the Linux
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
            if model.organizeShowsDocuments {
                OrganizeDocumentsList(model: model)
            } else {
                pages
            }
        }
        .accessibilityIdentifier("organize-grid")
    }

    /// The Pages view: every page as a card.
    private var pages: some View {
        let sources = model.store.pageSourceNames(baseName: model.baseBlockName)
        return ScrollView {
            LazyVGrid(columns: columns, spacing: 16) {
                ForEach(model.store.pageSlots, id: \.index) { slot in
                    OrganizePageCard(
                        number: slot.index + 1,
                        source: sources.indices.contains(slot.index) ? sources[slot.index] : nil,
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

    private func appendBlankPage(_ orientation: PageEdit.Orientation) {
        model.editPages(.insertBlank(at: model.store.pageSlots.count, orientation: orientation))
    }

    private var header: some View {
        HStack {
            Text("Organize pages").font(.headline)
            Picker("View", selection: Binding(get: { model.organizeShowsDocuments }, set: model.showOrganizeDocuments)) {
                Text("Documents").tag(true)
                Text("Pages").tag(false)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .frame(width: 180)
            .help(model.organizeShowsDocuments ? DocumentBlocks.documentsHint : DocumentBlocks.pagesHint)
            Text("Each change is one undo step.").foregroundStyle(.secondary)
            Spacer()
            Button("Add PDFs…", action: model.importPdfs)
                .help("Add every page of other PDFs at the end")
                .disabled(!model.store.canEditPages || model.isImporting)
            Button("Add blank A4 page") { appendBlankPage(.portrait) }
                .disabled(!model.store.canEditPages)
            Button("Add landscape A4 page") { appendBlankPage(.landscape) }
                .disabled(!model.store.canEditPages)
            if let progress = model.importProgress {
                ProgressView(value: Double(progress.completed), total: Double(max(progress.total, 1)))
                    .frame(width: 80)
                Text(progress.text).foregroundStyle(.secondary).monospacedDigit()
                Button("Cancel import", action: model.cancelImport)
                    .help("Stop adding the picked PDFs; nothing has changed yet")
            } else if model.store.pageWorkInFlight {
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
