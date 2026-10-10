import AppKit

/// Owns the one `ViewerViewModel` so the scene and the quit guard share it:
/// quitting with unsaved changes asks first, exactly like opening another
/// document does.
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = ViewerViewModel()

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        // A save in flight cannot be interrupted safely; let it land first.
        guard !model.isSaving else { return .terminateCancel }
        // Discard and Cancel answer synchronously; Save answers once the
        // bytes are on disk (or the write failed), after this returns — which
        // is what `.terminateLater` plus `reply(toApplicationShouldTerminate:)`
        // is for.
        var synchronousAnswer: Bool?
        var returned = false
        model.resolveUnsavedChanges { proceed in
            if returned {
                sender.reply(toApplicationShouldTerminate: proceed)
            } else {
                synchronousAnswer = proceed
            }
        }
        returned = true
        guard let synchronousAnswer else { return .terminateLater }
        return synchronousAnswer ? .terminateNow : .terminateCancel
    }
}
