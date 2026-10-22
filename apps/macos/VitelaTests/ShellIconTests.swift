import AppKit
import XCTest
@testable import Vitela

/// The icon contract Windows' `IconParityTests` holds: every drawing in
/// `assets/icons` is reachable from the shell and nothing in code points at a
/// missing file — here, also that the app bundle carries each one and AppKit
/// can draw it.
final class ShellIconTests: XCTestCase {
    private var sharedIcons: URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent() // VitelaTests
            .deletingLastPathComponent() // macos
            .deletingLastPathComponent() // apps
            .deletingLastPathComponent() // repository
            .appendingPathComponent("assets/icons")
    }

    func testEveryDrawingInTheSharedSetHasACaseAndNoCaseIsOrphaned() throws {
        let onDisk = try FileManager.default.contentsOfDirectory(atPath: sharedIcons.path)
            .filter { $0.hasSuffix(".svg") }
            .map { String($0.dropLast(4)) }
        XCTAssertEqual(Set(onDisk), Set(ShellIcon.allCases.map(\.fileName)))
    }

    func testTheAppBundlesEveryIconAsADrawableTemplate() {
        for icon in ShellIcon.allCases {
            XCTAssertNotNil(icon.url(), "\(icon.fileName).svg is not in the app bundle")
            let image = ShellIconImage.image(icon)
            XCTAssertNotNil(image, "\(icon.fileName).svg does not load")
            XCTAssertEqual(image?.isTemplate, true)
            XCTAssertGreaterThan(image?.size.width ?? 0, 0)
        }
    }
}
