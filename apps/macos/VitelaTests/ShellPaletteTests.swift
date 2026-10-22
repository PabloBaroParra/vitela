import AppKit
import XCTest
@testable import Vitela

/// The palette contract Windows' `ThemeParityTests` holds: the roles the
/// macOS shell paints with carry the very values Linux's `theme.rs` and
/// Windows' `Themes/Palette.xaml` define, in both schemes, and the colours
/// built from them follow the window's appearance.
final class ShellPaletteTests: XCTestCase {
    private var repository: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // VitelaTests
            .deletingLastPathComponent() // macos
            .deletingLastPathComponent() // apps
            .deletingLastPathComponent() // repository
    }

    private func linuxName(_ role: ShellPaletteRole) -> String {
        switch role {
        case .accent: return "vitela_accent"
        case .canvas: return "vitela_canvas"
        case .toolProtect: return "vitela_tool_protect"
        }
    }

    private func hex(_ text: String) -> UInt32? { UInt32(text.dropFirst(), radix: 16) }

    func testEveryRoleMatchesLinuxsPaletteInBothSchemes() throws {
        let source = try String(contentsOf: repository.appendingPathComponent("apps/linux-gtk/src/app/theme.rs"))
        for role in ShellPaletteRole.allCases {
            let name = linuxName(role)
            let pattern = try NSRegularExpression(pattern: "token\\(\"\(name)\", \"(#[0-9a-fA-F]{6})\", \"(#[0-9a-fA-F]{6})\"\\)")
            let match = try XCTUnwrap(pattern.firstMatch(in: source, range: NSRange(source.startIndex..., in: source)),
                                      "\(name) is not in theme.rs")
            let light = String(source[Range(match.range(at: 1), in: source)!])
            let dark = String(source[Range(match.range(at: 2), in: source)!])
            XCTAssertEqual(role.hex.light, hex(light), "\(name) light")
            XCTAssertEqual(role.hex.dark, hex(dark), "\(name) dark")
        }
    }

    func testAccentAndCanvasMatchWindowsPalette() throws {
        let xaml = try String(contentsOf: repository.appendingPathComponent("apps/windows/Pdf.Windows/Themes/Palette.xaml"))
        let dark = try XCTUnwrap(xaml.range(of: "x:Key=\"Dark\""))
        func value(_ key: String, in part: Substring) throws -> UInt32? {
            let marker = try XCTUnwrap(part.range(of: "x:Key=\"\(key)\" Color=\""), "\(key) is not in Palette.xaml")
            return hex(String(part[marker.upperBound...].prefix(7)))
        }
        for (role, key) in [(ShellPaletteRole.accent, "VitelaAccentBrush"), (.canvas, "VitelaCanvasBackgroundBrush")] {
            XCTAssertEqual(role.hex.light, try value(key, in: xaml[..<dark.lowerBound]), "\(key) light")
            XCTAssertEqual(role.hex.dark, try value(key, in: xaml[dark.upperBound...]), "\(key) dark")
        }
    }

    func testColoursResolveToTheSchemeOfTheAppearance() {
        for role in ShellPaletteRole.allCases {
            XCTAssertEqual(resolved(.vitela(role), .aqua), role.hex.light)
            XCTAssertEqual(resolved(.vitela(role), .darkAqua), role.hex.dark)
        }
    }

    /// Linux's `ICON_TINTS`: Protect's tile tint follows the scheme, the
    /// other tool accents read on both and stay put.
    func testOnlyTintsThePaletteNamesFollowTheScheme() {
        XCTAssertEqual(resolved(.vitelaTint(0x6366F1), .darkAqua), 0x818CF8)
        XCTAssertEqual(resolved(.vitelaTint(0x6366F1), .aqua), 0x6366F1)
        for tint: UInt32 in [0x14B8A6, 0xEC4899, 0x22C55E, 0xF59E0B] {
            XCTAssertNil(ShellPaletteRole.following(lightHex: tint))
            XCTAssertEqual(resolved(.vitelaTint(tint), .darkAqua), tint)
        }
    }

    private func resolved(_ color: NSColor, _ name: NSAppearance.Name) -> UInt32? {
        var result: UInt32?
        NSAppearance(named: name)!.performAsCurrentDrawingAppearance {
            guard let srgb = color.usingColorSpace(.sRGB) else { return }
            let channel = { (value: CGFloat) in UInt32((value * 255).rounded()) }
            result = channel(srgb.redComponent) << 16 | channel(srgb.greenComponent) << 8 | channel(srgb.blueComponent)
        }
        return result
    }
}
