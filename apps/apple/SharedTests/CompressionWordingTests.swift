import XCTest
@testable import Vitela

final class CompressionWordingTests: XCTestCase {
    private func result(before: Int, after: Int, refusals: [String] = []) -> CompressionResult {
        CompressionResult(bytes: Data(), beforeBytes: before, afterBytes: after, savedBytes: before - after, reduced: after < before, refusals: refusals)
    }

    func testSizesCountInPowersOfTen() {
        XCTAssertEqual(CompressionWording.humanSize(1), "1 byte")
        XCTAssertEqual(CompressionWording.humanSize(999), "999 bytes")
        XCTAssertEqual(CompressionWording.humanSize(1_500), "2 kB")
        XCTAssertEqual(CompressionWording.humanSize(2_340_000), "2.3 MB")
    }

    func testThePercentageIsTruncatedNeverRoundedUp() {
        XCTAssertEqual(CompressionWording.percentSmaller(result(before: 1_000, after: 503)), 49)
        XCTAssertEqual(CompressionWording.percentSmaller(result(before: 0, after: 0)), 0)
    }

    func testSummariesMatchWindows() {
        let reduced = result(before: 2_000_000, after: 1_000_000)
        XCTAssertEqual(
            CompressionWording.reductionSummary(reduced),
            "2.0 MB uncompressed → 1.0 MB compressed (50% smaller). Choose where to write it."
        )
        XCTAssertEqual(
            CompressionWording.writtenSummary(reduced, destination: "/tmp/a.pdf"),
            "Compressed PDF written to /tmp/a.pdf (2.0 MB → 1.0 MB, 50% smaller)."
        )
        XCTAssertEqual(
            CompressionWording.noGainSummary(result(before: 5_000, after: 5_000, refusals: ["a", "b"])),
            "Nothing to gain: this document compresses to the same 5 kB an uncompressed save writes, so no file was written. Not everything could be done: a; b."
        )
    }

    func testBalancedIsDescribedAsWindowsDoes() {
        XCTAssertEqual(CompressionWording.describe(.balanced).name, "Balanced")
    }

    func testTheOpenedFileSizeIsKnownForTheDialog() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]))
        XCTAssertNil(store.sourceByteCount)

        store.open(bytes: Data(repeating: 1, count: 1234))

        XCTAssertEqual(store.sourceByteCount, 1234)
    }
}
