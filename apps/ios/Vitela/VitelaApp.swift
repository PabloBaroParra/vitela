import SwiftUI

@main
struct VitelaApp: App {
    @StateObject private var model = ViewerViewModel()

    var body: some Scene {
        WindowGroup {
            ViewerRootView(model: model)
        }
    }
}
