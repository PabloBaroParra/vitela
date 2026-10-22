import AppKit
import SwiftUI

/// A `ShellIcon` as SwiftUI draws it: the bundled SVG as a template image,
/// so it takes the control's foreground colour (dimmed when disabled, white
/// on a selected toggle) the way Linux and Windows tint theirs by role.
struct ShellIconImage: View {
    let icon: ShellIcon
    var size: CGFloat = 16

    var body: some View {
        Group {
            if let image = Self.image(icon) {
                Image(nsImage: image)
                    .renderingMode(.template)
                    .resizable()
                    .aspectRatio(contentMode: .fit)
            } else {
                Color.clear
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }

    private static var cache: [ShellIcon: NSImage] = [:]

    /// The drawing, loaded once per icon; `nil` when it is missing from the
    /// bundle — a control then still has its title and tooltip.
    static func image(_ icon: ShellIcon, in bundle: Bundle = .main) -> NSImage? {
        if let cached = cache[icon] { return cached }
        guard let url = icon.url(in: bundle), let image = NSImage(contentsOf: url) else { return nil }
        image.isTemplate = true
        cache[icon] = image
        return image
    }
}

extension Label where Title == Text, Icon == ShellIconImage {
    /// A control's title with its shared drawing.
    init(_ title: String, shellIcon: ShellIcon) {
        self.init { Text(title) } icon: { ShellIconImage(icon: shellIcon) }
    }
}
