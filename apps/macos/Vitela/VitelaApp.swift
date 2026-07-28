import SwiftUI

@main
struct VitelaApp: App {
    @StateObject private var model = ViewerViewModel()

    var body: some Scene {
        WindowGroup("Vitela") {
            ViewerRootView(model: model)
                .frame(minWidth: 640, minHeight: 480)
        }
    }
}
