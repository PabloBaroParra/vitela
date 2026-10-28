import SwiftUI

/// Organize's document-wide controls, separate from its cards, like Linux's
/// `organize/header.rs`. They wrap instead of pushing Done/Save off screen.
/// Every action still uses the existing viewer use case and core guards.
struct OrganizeHeader: View {
    @ObservedObject var model: ViewerViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Organize pages").font(.headline)
                Spacer()
                Button("Done") { model.isOrganizing = false }
                    .keyboardShortcut(.cancelAction)
            }
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), alignment: .leading)], alignment: .leading, spacing: 8) {
                Picker("View", selection: Binding(get: { model.organizeShowsDocuments }, set: model.showOrganizeDocuments)) {
                    Text("Documents").tag(true)
                    Text("Pages").tag(false)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                .frame(width: 180)
                .help(model.organizeShowsDocuments ? DocumentBlocks.documentsHint : DocumentBlocks.pagesHint)
                Button("Add PDFs…", action: model.importPdfs)
                    .help("Add every page of other PDFs at the end")
                    .disabled(!model.store.canEditPages || model.isImporting)
                Button("Add blank A4 page") { appendBlankPage(.portrait) }
                    .disabled(!model.store.canEditPages)
                Button("Add landscape A4 page") { appendBlankPage(.landscape) }
                    .disabled(!model.store.canEditPages)
                HStack {
                    Button("Extract…", action: model.requestExtractPages)
                        .help("Save chosen pages as a new PDF (⇧⌘E)")
                        .disabled(!model.canRequestExtract)
                    Button("Split…", action: model.requestSplitPages)
                        .help("Cut this PDF into several new PDFs")
                        .disabled(!model.canRequestSplit)
                }
                HStack {
                    Button(action: model.undo) { Label("Undo", shellIcon: .undo) }
                        .labelStyle(.iconOnly)
                        .help("Undo (⌘Z)")
                        .disabled(!model.store.canUndo || model.store.pageWorkInFlight)
                    Button(action: model.redo) { Label("Redo", shellIcon: .redo) }
                        .labelStyle(.iconOnly)
                        .help("Redo (⇧⌘Z)")
                        .disabled(!model.store.canRedo || model.store.pageWorkInFlight)
                    Button { model.saveAs() } label: { Label("Save As…", shellIcon: .save) }
                        .help("Save as (⌘S)")
                        .disabled(model.store.document == nil || model.isSaving)
                }
            }
            Text("Each change is one undo step.").foregroundStyle(.secondary)
            if let progress = model.importProgress {
                HStack {
                    ProgressView(value: Double(progress.completed), total: Double(max(progress.total, 1)))
                        .frame(width: 80)
                    Text(progress.text).foregroundStyle(.secondary).monospacedDigit()
                    Button("Cancel import", action: model.cancelImport)
                        .help("Stop adding the picked PDFs; nothing has changed yet")
                }
            } else if model.store.pageWorkInFlight {
                ProgressView().controlSize(.small)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    private func appendBlankPage(_ orientation: PageEdit.Orientation) {
        model.editPages(.insertBlank(at: model.store.pageSlots.count, orientation: orientation))
    }
}
