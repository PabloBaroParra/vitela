import AppKit
import SwiftUI

extension NSColor {
    /// A palette role as a dynamic colour: AppKit resolves it again whenever
    /// the appearance changes, so a switch to Dark Mode repaints without a
    /// relaunch — Linux's live scheme switch in `theme.rs`.
    static func vitela(_ role: ShellPaletteRole) -> NSColor {
        NSColor(name: nil) { appearance in
            NSColor(rgbHex: role.hex(dark: appearance.isDark))
        }
    }

    /// A fixed tile tint, following the scheme when the palette has a role
    /// for it (`ShellPaletteRole.following`).
    static func vitelaTint(_ lightHex: UInt32) -> NSColor {
        ShellPaletteRole.following(lightHex: lightHex).map(vitela) ?? NSColor(rgbHex: lightHex)
    }

    convenience init(rgbHex hex: UInt32) {
        self.init(srgbRed: CGFloat(hex >> 16 & 0xFF) / 255,
                  green: CGFloat(hex >> 8 & 0xFF) / 255,
                  blue: CGFloat(hex & 0xFF) / 255,
                  alpha: 1)
    }
}

extension NSAppearance {
    /// Whether this appearance draws the dark half of the palette.
    var isDark: Bool { bestMatch(from: [.aqua, .darkAqua]) == .darkAqua }
}

extension Color {
    static func vitela(_ role: ShellPaletteRole) -> Color { Color(nsColor: .vitela(role)) }

    /// Vitela's accent, which the shell's chrome (drop targets, the current
    /// page and search row, Edit's tile) uses where it once took the
    /// system's: the other shells paint these violet whatever the desktop's
    /// accent is. On-page marks (selection, outlines) keep their own fixed
    /// colours, as Linux's `selection.rs` does.
    static let vitelaAccent = vitela(.accent)
}
