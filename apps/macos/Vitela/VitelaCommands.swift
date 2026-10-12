import SwiftUI

/// The menu bar: File ▸ Open/Save/Print, Edit ▸ Undo/Redo, the Go menu and
/// View ▸ zoom, with the standard shortcuts. Undo and Redo replace the system items because the document's
/// history lives in the core's EditLog, not in an `NSUndoManager`.
struct VitelaCommands: Commands {
    @ObservedObject var model: ViewerViewModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Open…", action: model.selectDocument)
                .keyboardShortcut("o", modifiers: .command)
        }
        CommandGroup(replacing: .saveItem) {
            Button("Save As…") { model.saveAs() }
                .keyboardShortcut("s", modifiers: .command)
                .disabled(model.store.document == nil || model.isSaving)
        }
        CommandGroup(replacing: .printItem) {
            Button("Print…", action: model.printDocument)
                .keyboardShortcut("p", modifiers: .command)
                .disabled(model.store.document == nil)
        }
        CommandGroup(replacing: .undoRedo) {
            Button("Undo", action: model.undo)
                .keyboardShortcut("z", modifiers: .command)
                .disabled(!model.store.canUndo)
            Button("Redo", action: model.redo)
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!model.store.canRedo)
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
        }
        CommandGroup(before: .toolbar) {
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
