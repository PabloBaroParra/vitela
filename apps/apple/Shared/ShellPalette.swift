// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The roles of Vitela's palette an Apple shell paints with, two values each
/// — the light and dark halves of Linux's `theme.rs` `PALETTE` and Windows'
/// `Themes/Palette.xaml` (pinned there by `ThemeParityTests`). Native chrome
/// (window, sidebars, controls) stays the system's; these are the colours the
/// other shells make Vitela's own.
enum ShellPaletteRole: CaseIterable {
    /// The accent as text, icon, outline or tint; lightens on dark so it
    /// still reads (`vitela_accent`, `VitelaAccentBrush`).
    case accent
    /// The recessed surface the pages sit on (`vitela_canvas`,
    /// `VitelaCanvasBackgroundBrush`).
    case canvas
    /// Protect's tile tint, too dim on the dark surfaces to keep
    /// (`vitela_tool_protect`).
    case toolProtect

    /// `0xRRGGBB` in the light and dark scheme.
    var hex: (light: UInt32, dark: UInt32) {
        switch self {
        case .accent: return (0x6B4EFF, 0xA882FF)
        case .canvas: return (0xE9E6EC, 0x1C1C1C)
        case .toolProtect: return (0x6366F1, 0x818CF8)
        }
    }

    func hex(dark: Bool) -> UInt32 { dark ? hex.dark : hex.light }

    /// The role a fixed tint follows when the scheme changes, by the light
    /// value the caller names it with — Linux's `ICON_TINTS`. A tint not
    /// listed (the other tool accents) reads on both schemes and stays put.
    static func following(lightHex: UInt32) -> ShellPaletteRole? {
        allCases.first { $0.hex.light == lightHex }
    }
}
