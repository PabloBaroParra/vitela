import SwiftUI
import UniformTypeIdentifiers

/// Organize's Documents view: one card per run of pages from the same PDF,
/// with a slim drop gap before, between and after them — Linux's reason: a
/// gap *is* the position, so the highlighted one says where the block lands.
struct OrganizeDocumentsList: View {
    @ObservedObject var model: ViewerViewModel
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        let rows = model.store.documentBlockRows(baseName: model.baseBlockName)
        let enabled = model.store.canEditPages && !model.isImporting
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                BlockDropGap(model: model, slot: 0)
                ForEach(Array(rows.enumerated()), id: \.offset) { position, row in
                    OrganizeDocumentCard(
                        row: row,
                        position: position,
                        total: rows.count,
                        cover: model.store.thumbnails[row.block.start],
                        enabled: enabled,
                        onAction: { model.editBlock(position, $0) }
                    )
                    .onDrag {
                        guard model.store.canEditPages else { return NSItemProvider() }
                        return NSItemProvider(object: model.beginBlockDrag(position: position) as NSString)
                    }
                    .onAppear { model.renderThumbnail(page: row.block.start, scale: Double(displayScale)) }
                    // A relayout reuses positions; only a new identity
                    // re-fires the cover's `onAppear`, as in the Pages view.
                    .id("\(model.store.generation)-\(position)")
                    BlockDropGap(model: model, slot: position + 1)
                }
            }
            .padding()
        }
        .accessibilityIdentifier("organize-documents")
    }
}

/// One block: its first page with the edges of the next two behind it, what
/// it is called, where it sits, and what can be done to it as a whole.
private struct OrganizeDocumentCard: View {
    let row: DocumentBlockRow
    let position: Int
    let total: Int
    let cover: RenderedPage?
    let enabled: Bool
    let onAction: (ViewerStore.BlockAction) -> Void

    private static let coverSize = CGSize(width: 96, height: 124)

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            sheets
            VStack(alignment: .leading, spacing: 4) {
                Text(row.title).font(.headline).lineLimit(1).truncationMode(.middle)
                Text(row.meta).foregroundStyle(.secondary)
                HStack(spacing: 2) {
                    action("arrow.up", "Move up", .move(toSlot: position - 1)).disabled(position == 0)
                    action("arrow.down", "Move down", .move(toSlot: position + 2)).disabled(position + 1 >= total)
                    action("rotate.left", "Rotate left", .rotateLeft)
                    action("rotate.right", "Rotate right", .rotateRight)
                    // A PDF needs at least one page.
                    action("trash", "Delete", .delete).disabled(total == 1)
                }
                .buttonStyle(.borderless)
                .disabled(!enabled)
            }
            Spacer(minLength: 0)
        }
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("\(row.title), \(row.meta), document \(position + 1) of \(total)")
    }

    private func action(_ symbol: String, _ label: String, _ action: ViewerStore.BlockAction) -> some View {
        Button { onAction(action) } label: { Image(systemName: symbol) }
            .help(label)
            .accessibilityLabel("\(label): \(row.title)")
    }

    private var sheets: some View {
        ZStack(alignment: .topLeading) {
            ForEach((1...2).reversed().filter { row.block.count > $0 }, id: \.self) { depth in
                RoundedRectangle(cornerRadius: 2)
                    .fill(Color.white)
                    .shadow(radius: 1)
                    .frame(width: Self.coverSize.width, height: Self.coverSize.height)
                    .offset(x: CGFloat(depth * 4), y: CGFloat(depth * 4))
            }
            Group {
                if let image = cover?.nsImage {
                    Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
                } else {
                    ProgressView()
                }
            }
            .frame(width: Self.coverSize.width, height: Self.coverSize.height)
            .background(Color.white)
            .shadow(radius: 1)
        }
        .frame(width: Self.coverSize.width + 8, height: Self.coverSize.height + 8, alignment: .topLeading)
    }
}

/// A drop target between cards; lit while a card is dragged over it.
private struct BlockDropGap: View {
    @ObservedObject var model: ViewerViewModel
    let slot: Int
    @State private var targeted = false

    var body: some View {
        RoundedRectangle(cornerRadius: 2)
            .fill(targeted ? Color.accentColor : Color.clear)
            .frame(height: 6)
            .padding(.vertical, 3)
            .onDrop(of: [.plainText], delegate: BlockDropTarget(model: model, slot: slot, targeted: $targeted))
    }
}

/// Reads the drag's token back out of the payload; the move is the view
/// model's.
private struct BlockDropTarget: DropDelegate {
    let model: ViewerViewModel
    let slot: Int
    @Binding var targeted: Bool

    func validateDrop(info: DropInfo) -> Bool {
        model.organizeBlockDrag != nil && model.store.canEditPages && info.hasItemsConforming(to: [.plainText])
    }

    func dropEntered(info: DropInfo) { targeted = validateDrop(info: info) }
    func dropExited(info: DropInfo) { targeted = false }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: validateDrop(info: info) ? .move : .forbidden)
    }

    func performDrop(info: DropInfo) -> Bool {
        targeted = false
        guard validateDrop(info: info), let provider = info.itemProviders(for: [.plainText]).first else { return false }
        _ = provider.loadObject(ofClass: NSString.self) { token, _ in
            guard let token = token as? String else { return }
            DispatchQueue.main.async { model.dropBlock(token: token, atSlot: slot) }
        }
        return true
    }
}
