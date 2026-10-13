import SwiftUI

/// The menu bar: File ▸ New/Open/Save/Protect/Sign/Compress/Extract/Split/
/// Export/Print/Share, Edit ▸ Undo/Redo, the Go menu and View ▸ Pages and zoom, with the standard shortcuts. Undo and Redo replace the system items because the document's
/// history lives in the core's EditLog, not in an `NSUndoManager`.
struct VitelaCommands: Commands {
    @ObservedObject var model: ViewerViewModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("New", action: model.newDocument)
                .keyboardShortcut("n", modifiers: .command)
            Button("Open…", action: model.selectDocument)
                .keyboardShortcut("o", modifiers: .command)
            Menu("Open Recent") {
                ForEach(Array(model.recentDocuments.prefix(10)), id: \.path) { document in
                    Button(document.name) { model.openRecent(document) }
                }
                if !model.recentDocuments.isEmpty {
                    Divider()
                }
                Button("Clear Menu", action: model.clearRecents)
                    .disabled(model.recentDocuments.isEmpty)
            }
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save As…") { model.saveAs() }
                .keyboardShortcut("s", modifiers: .command)
                .disabled(model.store.document == nil || model.isSaving)
            Button("Extract Pages…", action: model.requestExtractPages)
                .keyboardShortcut("e", modifiers: [.command, .shift])
                .disabled(model.store.document == nil || model.isSaving)
            Button("Split PDF…", action: model.requestSplitPages)
                .disabled(model.store.pageSlots.count < 2 || model.isSaving)
            Button("Export Pages as Images…", action: model.requestExportImages)
                .disabled(model.store.document == nil || model.isSaving)
            Button("Compress…", action: model.requestCompress)
                .disabled(model.store.document == nil || model.isSaving)
            Button("Protect with Password…", action: model.requestProtect)
                .disabled(model.store.document == nil || model.isSaving)
            Button("Sign with Certificate…", action: model.signWithCertificateFile)
                .disabled(model.store.document == nil || model.isSaving || model.store.signingRefusal != nil)
        }
        CommandGroup(replacing: .printItem) {
            Button("Print…", action: model.printDocument)
                .keyboardShortcut("p", modifiers: .command)
                .disabled(model.store.document == nil)
            Button("Share…", action: model.share)
                .disabled(!model.canShare)
        }
        CommandGroup(replacing: .undoRedo) {
            Button("Undo", action: model.undo)
                .keyboardShortcut("z", modifiers: .command)
                .disabled(!model.store.canUndo || model.store.pageWorkInFlight)
            Button("Redo", action: model.redo)
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!model.store.canRedo || model.store.pageWorkInFlight)
        }
        CommandMenu("Go") {
            // Previous/Next and Go to Page use Preview's shortcuts; First/Last
            // map the Windows shell's Ctrl+Home/End onto Command.
            Button("Previous Page") { model.store.stepPage(by: -1) }
                .keyboardShortcut(.upArrow, modifiers: [.command, .option])
                .disabled(!model.store.canGoToPreviousPage)
            Button("Next Page") { model.store.stepPage(by: 1) }
                .keyboardShortcut(.downArrow, modifiers: [.command, .option])
                .disabled(!model.store.canGoToNextPage)
            Divider()
            Button("First Page", action: model.store.goToFirstPage)
                .keyboardShortcut(.home, modifiers: .command)
                .disabled(!model.store.canGoToPreviousPage)
            Button("Last Page", action: model.store.goToLastPage)
                .keyboardShortcut(.end, modifiers: .command)
                .disabled(!model.store.canGoToNextPage)
            Divider()
            Button("Go to Page…", action: model.requestGoToPage)
                .keyboardShortcut("g", modifiers: [.command, .option])
                .disabled(model.store.pageCount == 0)
            Divider()
            Button("Previous Annotation") { model.store.stepAnnotation(forward: false) }
                .keyboardShortcut("[", modifiers: [.command, .option])
                .disabled(!model.store.canStepAnnotations || model.isOrganizing)
            Button("Next Annotation") { model.store.stepAnnotation(forward: true) }
                .keyboardShortcut("]", modifiers: [.command, .option])
                .disabled(!model.store.canStepAnnotations || model.isOrganizing)
        }
        CommandGroup(before: .toolbar) {
            // Preview's shortcut for its thumbnail sidebar.
            Button(model.showsPages ? "Hide Pages" : "Show Pages") { model.showsPages.toggle() }
                .keyboardShortcut("2", modifiers: [.command, .option])
            Divider()
            // ⌘= rather than ⌘+: it is the same key without Shift.
            Button("Zoom In") { model.store.stepZoom(1) }
                .keyboardShortcut("=", modifiers: .command)
                .disabled(model.store.document == nil)
            Button("Zoom Out") { model.store.stepZoom(-1) }
                .keyboardShortcut("-", modifiers: .command)
                .disabled(model.store.document == nil)
            Button("Actual Size") { model.store.setZoom(1) }
                .keyboardShortcut("0", modifiers: .command)
                .disabled(model.store.document == nil)
            Button("Fit Width") { model.store.setZoomMode(.fitWidth) }
                .disabled(model.store.document == nil)
            Button("Fit Page") { model.store.setZoomMode(.fitPage) }
                .keyboardShortcut("9", modifiers: .command)
                .disabled(model.store.document == nil)
            Divider()
        }
    }
}
