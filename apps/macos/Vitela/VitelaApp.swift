import SwiftUI

@main
struct VitelaApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate

    var body: some Scene {
        WindowGroup("Vitela") {
            ViewerRootView(model: appDelegate.model)
                .frame(minWidth: 640, minHeight: 480)
        }
        .commands { VitelaCommands(model: appDelegate.model) }
    }
}
