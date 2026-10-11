import AppKit

/// Owns the one `ViewerViewModel` so the scene, the quit guard and Finder's
/// open requests share it: quitting with unsaved changes asks first, exactly
/// like opening another document does.
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = ViewerViewModel()

    /// Finder's double-click and Open With, and files dropped on the Dock
    /// icon — the PDF types are declared in `Info.plist`.
    func application(_ application: NSApplication, open urls: [URL]) {
        model.openExternalFiles(urls)
    }

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
