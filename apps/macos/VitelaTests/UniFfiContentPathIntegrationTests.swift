import XCTest
@testable import Vitela

/// Vector paths (B25 T-206) against the real core: a frame and a rule read
/// back as paths, and a press on the frame moves then reshapes it through
/// the shell's page-work pipeline — `core/pdf-ffi/tests/content_paths.rs`'
/// fixture, as Windows' `PathEditSmoke` drives it.
final class UniFfiContentPathIntegrationTests: XCTestCase {
    /// One page with a stroked frame at (319, 135) 251 × 125, a rule at
    /// y = 120 and a clip, which is never an item.
    static func framedPdf() -> Data {
        var pdf = Data("%PDF-1.7\n".utf8)
        var offsets: [Int] = []
        func object(_ body: Data) {
            offsets.append(pdf.count)
            pdf.append(Data("\(offsets.count) 0 obj\n".utf8))
            pdf.append(body)
            pdf.append(Data("\nendobj\n".utf8))
        }
        let content = Data("0.5 w 319 135 251 125 re S\n319 120 m 570 120 l S\nq 0 0 50 50 re W n Q".utf8)
        object(Data("<< /Type /Catalog /Pages 2 0 R >>".utf8))
        object(Data("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".utf8))
        object(Data("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 4 0 R >>".utf8))
        object(Data("<< /Length \(content.count) >>\nstream\n".utf8) + content + Data("\nendstream".utf8))
        let xref = pdf.count
        pdf.append(Data("xref\n0 \(offsets.count + 1)\n0000000000 65535 f \n".utf8))
        for offset in offsets { pdf.append(Data(String(format: "%010d 00000 n \n", offset).utf8)) }
        pdf.append(Data("trailer\n<< /Size \(offsets.count + 1) /Root 1 0 R >>\nstartxref\n\(xref)\n%%EOF\n".utf8))
        return pdf
    }

    func testAFrameIsSelectedMovedAndReshapedAgainstTheRealCore() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: Self.framedPdf())
        XCTAssertEqual(store.state, .loaded)
        store.setContentEditMode(true)
        func read() throws -> [ContentPath] {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0]).paths
        }
        func drag(from: PdfPoint, to: PdfPoint) throws {
            XCTAssertTrue(store.beginContentPress(page: 0, at: from, zoom: 1))
            if case let .move(request) = store.endContentDrag(at: to) {
                store.applyPageWork(store.pageWorkResult(for: request), for: request)
                _ = try read()
            }
        }
        let paths = try read()
        XCTAssertEqual(paths.count, 2, "the frame and the rule; the clip is not an item")
        let frame = try XCTUnwrap(paths.first { $0.bbox.height > 1 })
        XCTAssertEqual(frame.paint, .stroke)
        XCTAssertEqual(frame.bbox.x, 319, accuracy: 0.5)
        XCTAssertEqual(frame.bbox.width, 251, accuracy: 0.5)

        // Its left border: a click selects, a drag moves it 20 left, 10 up.
        try drag(from: PdfPoint(x: 319, y: 200), to: PdfPoint(x: 319, y: 200))
        XCTAssertEqual(store.selectedContentPath?.id, frame.id)
        try drag(from: PdfPoint(x: 319, y: 200), to: PdfPoint(x: 299, y: 210))
        XCTAssertTrue(store.canUndo)
        let moved = try XCTUnwrap(store.selectedContentPath, "re-selected at its new box")
        XCTAssertEqual(moved.bbox.x, 299, accuracy: 0.5)
        XCTAssertEqual(moved.bbox.y, 145, accuracy: 0.5)

        // Straight away, no save: its top-right corner reshapes it.
        try drag(from: PdfPoint(x: moved.bbox.x + moved.bbox.width, y: moved.bbox.y + moved.bbox.height),
                 to: PdfPoint(x: moved.bbox.x + moved.bbox.width + 30, y: moved.bbox.y + moved.bbox.height + 20))
        let reshaped = try XCTUnwrap(store.selectedContentPath)
        XCTAssertEqual(reshaped.id, frame.id)
        XCTAssertEqual(reshaped.bbox.width, 281, accuracy: 0.5)
        XCTAssertEqual(reshaped.bbox.height, 145, accuracy: 0.5)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        let undone = try XCTUnwrap(try read().first { $0.id == frame.id })
        XCTAssertEqual(undone.bbox.x, 319, accuracy: 0.5, "the core folds move and reshape into one undo step")
        XCTAssertEqual(undone.bbox.width, 251, accuracy: 0.5)
    } }
}
