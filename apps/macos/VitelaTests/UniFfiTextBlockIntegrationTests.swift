import XCTest
@testable import Vitela

/// Text blocks (B25 T-207) against the real core: a line written as two
/// `Tj`s reads back as one block, and retyping it is one undo step that
/// leaves all of the text in its first run.
final class UniFfiTextBlockIntegrationTests: XCTestCase {
    /// One page whose only line is two Helvetica `Tj`s side by side.
    static func splitLinePdf() -> Data {
        var pdf = Data("%PDF-1.7\n".utf8)
        var offsets: [Int] = []
        func object(_ body: Data) {
            offsets.append(pdf.count)
            pdf.append(Data("\(offsets.count) 0 obj\n".utf8))
            pdf.append(body)
            pdf.append(Data("\nendobj\n".utf8))
        }
        let content = Data("BT /F1 12 Tf 72 700 Td (PRESUPUES) Tj (TO 3) Tj ET".utf8)
        object(Data("<< /Type /Catalog /Pages 2 0 R >>".utf8))
        object(Data("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".utf8))
        object(Data("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".utf8))
        object(Data("<< /Length \(content.count) >>\nstream\n".utf8) + content + Data("\nendstream".utf8))
        object(Data("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".utf8))
        let xref = pdf.count
        pdf.append(Data("xref\n0 \(offsets.count + 1)\n0000000000 65535 f \n".utf8))
        for offset in offsets { pdf.append(Data(String(format: "%010d 00000 n \n", offset).utf8)) }
        pdf.append(Data("trailer\n<< /Size \(offsets.count + 1) /Root 1 0 R >>\nstartxref\n\(xref)\n%%EOF\n".utf8))
        return pdf
    }

    func testASplitLineIsOneBlockAndRetypesAsOneStep() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: Self.splitLinePdf())
        XCTAssertEqual(store.state, .loaded)
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let content = try read()
        XCTAssertEqual(content.textRuns.count, 2)
        let block = try XCTUnwrap(content.textBlocks.first)
        XCTAssertEqual(content.textBlocks.count, 1)
        XCTAssertTrue(block.isMultiRun)
        XCTAssertEqual(block.text, "PRESUPUESTO 3")

        // A press on the second run opens the whole block.
        let second = content.textRuns[1]
        let point = PdfPoint(x: second.bbox.x + second.bbox.width / 2, y: second.bbox.y + second.bbox.height / 2)
        XCTAssertTrue(store.beginContentPress(page: 0, at: point, zoom: 1))
        guard case let .open(opened) = store.endContentDrag(at: point) else { return XCTFail("a click opens") }
        XCTAssertEqual(opened, block)

        let request = try XCTUnwrap(store.beginContentEdit(ViewerStore.textRetype(opened, to: "PRESUPUESTO 4")))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
        XCTAssertTrue(store.canUndo)
        let retyped = try read()
        XCTAssertEqual(retyped.textRuns.map(\.text), ["PRESUPUESTO 4"])

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        XCTAssertEqual(try read().textRuns.map(\.text), ["PRESUPUES", "TO 3"], "one undo restores both runs")
    } }
}

extension UniFfiTextBlockIntegrationTests {
    /// #506: a moved line opens and retypes before any save — the core
    /// writes the pending move into an in-memory base and retries.
    func testAMovedLineRetypesBeforeASaveThroughTheCheckpoint() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: Self.splitLinePdf())
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let document = try XCTUnwrap(store.document)
        XCTAssertFalse(store.client.contentEditsCanCheckpoint(document: document), "nothing queued to checkpoint yet")
        let block = try XCTUnwrap(try read().textBlocks.first)
        let move = try XCTUnwrap(store.beginContentEdit(ViewerStore.textMove(block, to: PdfRect(x: block.bbox.x, y: block.bbox.y - 40,
                                                                                               width: block.bbox.width, height: block.bbox.height))))
        store.applyPageWork(store.pageWorkResult(for: move), for: move)
        let moved = try XCTUnwrap(try read().textBlocks.first)
        XCTAssertTrue(store.client.contentEditsCanCheckpoint(document: document), "an unencrypted file opened from bytes, content edits only")

        let point = PdfPoint(x: moved.bbox.x + 5, y: moved.bbox.y + moved.bbox.height / 2)
        XCTAssertTrue(store.beginContentPress(page: 0, at: point, zoom: 1))
        guard case let .open(opened) = store.endContentDrag(at: point) else { return XCTFail("the moved line opens") }
        let retype = try XCTUnwrap(store.beginContentEdit(ViewerStore.textRetype(opened, to: "PRESUPUESTO 5")))
        store.applyPageWork(store.pageWorkResult(for: retype), for: retype)
        let after = try read()
        XCTAssertEqual(after.textRuns.map(\.text), ["PRESUPUESTO 5"], store.editStatus)
        XCTAssertEqual(try XCTUnwrap(after.textRuns.first).bbox.y, moved.bbox.y, accuracy: 1, "retyped where it was moved")
    } }
}
