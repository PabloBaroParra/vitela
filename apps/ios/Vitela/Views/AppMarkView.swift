import SwiftUI

/// The Vitela mark, shown in the page area while no document is open —
/// mirrors macOS's `AppMarkView` and the empty state of the GTK4 and WinUI
/// shells, including the 96pt size. Unlike macOS (which loads the SVG file
/// straight from the bundle via `NSImage(contentsOf:)`), UIKit has no
/// runtime SVG decoder, so the two light/dark marks are vendored into
/// `Assets.xcassets` as vector-preserving image sets instead — Xcode's
/// asset compiler rasterizes them at build time. `Image(_:)` still picks the
/// file by `colorScheme` explicitly rather than relying on the catalog's own
/// dark-mode variant matching, for the same reason as macOS: the navy fill
/// reads on a light background and disappears on a dark one, so the shell
/// must choose the file, not tint one at runtime.
struct AppMarkView: View {
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Image(colorScheme == .dark ? Self.darkAssetName : Self.lightAssetName)
            .resizable()
            .aspectRatio(contentMode: .fit)
            .frame(width: Self.edge, height: Self.edge)
            .accessibilityHidden(true)
    }

    private static let edge: CGFloat = 96
    private static let lightAssetName = "vitela-app-mark"
    private static let darkAssetName = "vitela-app-mark-dark"
}
