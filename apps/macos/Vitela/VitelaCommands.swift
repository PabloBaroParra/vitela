import SwiftUI

/// The menu bar: File ▸ Open/Save and Edit ▸ Undo/Redo, with the standard
/// shortcuts. Undo and Redo replace the system items because the document's
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
        CommandGroup(replacing: .undoRedo) {
            Button("Undo", action: model.undo)
                .keyboardShortcut("z", modifiers: .command)
                .disabled(!model.store.canUndo)
            Button("Redo", action: model.redo)
                .keyboardShortcut("z", modifiers: [.command, .shift])
                .disabled(!model.store.canRedo)
        }
    }
}
