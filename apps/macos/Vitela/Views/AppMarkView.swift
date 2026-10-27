import SwiftUI

/// The Vitela mark — mirrors the GTK4 and WinUI shells (`app/brand.rs`,
/// `MainWindow.xaml`'s `AppMarkImageSource`), with a 96pt default (24pt in
/// the persistent rail) and the same two light/dark SVGs from `assets/brand/`: the navy fill
/// reads on a light background and disappears on a dark one, and the files
/// carry a literal fill rather than `currentColor`, so the shell picks the
/// file instead of tinting one at runtime.
struct AppMarkView: View {
    @Environment(\.colorScheme) private var colorScheme
    var edge: CGFloat = 96

    var body: some View {
        let mark = colorScheme == .dark ? Self.dark : Self.light
        Group {
            if let mark {
                Image(nsImage: mark)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
                    .frame(width: edge, height: edge)
            }
        }
        .accessibilityHidden(true)
    }

    // Loaded once per variant, not per appearance change: the SVG never
    // changes underneath the running app, only which of the two is on screen.
    private static let light = loadMark(named: "vitela-app-mark")
    private static let dark = loadMark(named: "vitela-app-mark-dark")

    private static func loadMark(named name: String) -> NSImage? {
        guard let url = Bundle.main.url(forResource: name, withExtension: "svg") else { return nil }
        return NSImage(contentsOf: url)
    }
}
