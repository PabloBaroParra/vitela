import Foundation

/// Whether this process runs inside the App Sandbox — the Mac App Store
/// build. The sandbox gives every process its container's identifier in the
/// environment; the Developer ID and development builds run without one.
enum AppSandbox {
    static var isActive: Bool { isActive(environment: ProcessInfo.processInfo.environment) }

    static func isActive(environment: [String: String]) -> Bool {
        environment["APP_SANDBOX_CONTAINER_ID"] != nil
    }
}
