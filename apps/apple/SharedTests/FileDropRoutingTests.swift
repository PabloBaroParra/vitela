import XCTest
@testable import Vitela

final class FileDropRoutingTests: XCTestCase {
    func testAPdfIsRecognisedByItsSignatureNotItsName() {
        XCTAssertEqual(DroppedFileKind.classify(Data("%PDF-1.7\n%âãÏÓ".utf8)), .document)
    }

    func testAnythingElseIsUnsupported() {
        XCTAssertEqual(DroppedFileKind.classify(Data([0x89, 0x50, 0x4E, 0x47])), .unsupported, "PNG")
        XCTAssertEqual(DroppedFileKind.classify(Data("%PDF".utf8)), .unsupported, "a truncated signature")
        XCTAssertEqual(DroppedFileKind.classify(Data()), .unsupported)
        XCTAssertEqual(DroppedFileKind.classify(Data(" %PDF-1.4".utf8)), .unsupported, "the signature must lead")
    }

    func testOnlyTheFirstLocalFileOfADropIsTaken() {
        let first = URL(fileURLWithPath: "/tmp/a.pdf")
        let second = URL(fileURLWithPath: "/tmp/b.pdf")
        let remote = URL(string: "https://example.com/c.pdf")!

        XCTAssertEqual(DroppedFileKind.firstFile([remote, first, second]), first)
        XCTAssertNil(DroppedFileKind.firstFile([remote]))
        XCTAssertNil(DroppedFileKind.firstFile([]))
    }
}
