import AppKit
import XCTest
@testable import Vitela

/// Edit Content against the real core: lines retyped, moved and added,
/// images moved, resized, deleted, replaced and added — split from
/// `UniFfiOutputIntegrationTests`, which keeps what the reader writes out.
final class UniFfiContentEditIntegrationTests: XCTestCase {}

extension UniFfiContentEditIntegrationTests {
    /// A line the sample paints is retyped against the real core, through
    /// the same page-work pipeline the shell runs, and reads back changed.
    func testRetypingALineOfTheSampleChangesWhatThePageSays() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        XCTAssertTrue(store.editingAllowed)
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let content = try read()
        let line = try XCTUnwrap(content.textRuns.first { !$0.substitutesFont && $0.text.count > 3 }, "the sample paints no simple-font line")
        // A prefix of the line: every character is one its font already shows.
        let shorter = String(line.text.prefix(3))

        let request = try XCTUnwrap(store.beginContentEdit(.retypeText(line, to: shorter)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertTrue(store.canUndo)
        XCTAssertTrue(try read().textRuns.contains { $0.text == shorter })
    } }
}

extension UniFfiContentEditIntegrationTests {
    /// A line of the sample moved against the real core lands where it was
    /// dropped, and is refused for retyping until the document is saved.
    func testMovingALineOfTheSampleMovesItAndHoldsItsRetype() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.setContentEditMode(true)
        func read() throws -> PageContent {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0])
        }
        let line = try XCTUnwrap(try read().textRuns.first { !$0.substitutesFont })
        let to = PdfRect(x: line.bbox.x + 20, y: line.bbox.y - 30, width: line.bbox.width, height: line.bbox.height)

        let request = try XCTUnwrap(store.beginContentEdit(.moveText(line, to: to)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertTrue(store.canUndo)
        let moved = try XCTUnwrap(try read().textRuns.first { $0.id == line.id })
        XCTAssertEqual(moved.bbox.x, to.x, accuracy: 1)
        XCTAssertEqual(moved.bbox.y, to.y, accuracy: 1)
        XCTAssertTrue(store.client.textRunHasPendingMove(document: try XCTUnwrap(store.document), run: moved))
    } }
}

extension UniFfiContentEditIntegrationTests {
    /// A new line added against the real core is painted by the page: it
    /// reads back as one of the page's own runs, in its own Helvetica.
    func testAddingALineToTheSampleMakesItPartOfThePage() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: try Self.sampleBytes())
        store.setContentEditMode(true)

        let (problem, request) = store.beginTextInsert(page: 0, at: PdfPoint(x: 72, y: 120), text: "Vitela was here", size: "14")
        XCTAssertNil(problem)
        let work = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: work), for: work)
        XCTAssertTrue(store.canUndo)

        let read = try XCTUnwrap(store.beginPageContent(page: 0))
        store.applyPageContent(store.pageContentResult(for: read), for: read)
        let added = try XCTUnwrap(store.pageContent[0]?.textRuns.first { $0.text == "Vitela was here" })
        XCTAssertEqual(added.fontKind, .standard14)
        XCTAssertTrue(added.resourceFontName.hasPrefix("VitelaText"))
    } }
}

extension UniFfiContentEditIntegrationTests {
    /// One page painting one 1×1 red image at (72, 600), 100 × 50 points —
    /// unfiltered, or Flate-compressed with `flate`.
    static func onePhotoPdf(flate: Bool = false) -> Data {
        var pdf = Data("%PDF-1.7\n".utf8)
        var offsets: [Int] = []
        func object(_ body: Data) {
            offsets.append(pdf.count)
            pdf.append(Data("\(offsets.count) 0 obj\n".utf8))
            pdf.append(body)
            pdf.append(Data("\nendobj\n".utf8))
        }
        let content = Data("q 100 0 0 50 72 600 cm /Im1 Do Q".utf8)
        object(Data("<< /Type /Catalog /Pages 2 0 R >>".utf8))
        object(Data("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".utf8))
        object(Data("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>".utf8))
        object(Data("<< /Length \(content.count) >>\nstream\n".utf8) + content + Data("\nendstream".utf8))
        let pixel = Data([0xFF, 0x00, 0x00])
        let samples = flate ? zlib(pixel) : pixel
        let filter = flate ? " /Filter /FlateDecode" : ""
        object(Data("<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8\(filter) /Length \(samples.count) >>\nstream\n".utf8)
            + samples + Data("\nendstream".utf8))
        let xref = pdf.count
        pdf.append(Data("xref\n0 \(offsets.count + 1)\n0000000000 65535 f \n".utf8))
        for offset in offsets { pdf.append(Data(String(format: "%010d 00000 n \n", offset).utf8)) }
        pdf.append(Data("trailer\n<< /Size \(offsets.count + 1) /Root 1 0 R >>\nstartxref\n\(xref)\n%%EOF\n".utf8))
        return pdf
    }

    /// RFC 1950 zlib: Foundation's raw DEFLATE with its header and Adler-32.
    static func zlib(_ data: Data) -> Data {
        let deflated = (try? (data as NSData).compressed(using: .zlib) as Data) ?? Data()
        var a: UInt32 = 1, b: UInt32 = 0
        for byte in data { a = (a + UInt32(byte)) % 65521; b = (b + a) % 65521 }
        let adler = (b << 16) | a
        return Data([0x78, 0x9C]) + deflated + Data([UInt8(adler >> 24), UInt8(adler >> 16 & 0xFF), UInt8(adler >> 8 & 0xFF), UInt8(adler & 0xFF)])
    }

    /// An image the page paints is found, moved, resized and deleted against
    /// the real core, each as one undoable step that reads back.
    func testAnImageIsMovedResizedAndDeleted() throws { try offMain {
        let store = ViewerStore(client: UniFfiPdfCoreClient())
        store.open(bytes: Self.onePhotoPdf())
        XCTAssertEqual(store.state, .loaded)
        store.setContentEditMode(true)
        func read() throws -> [ContentImage] {
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            return try XCTUnwrap(store.pageContent[0]).images
        }
        func perform(_ edit: ContentEdit) throws {
            let request = try XCTUnwrap(store.beginContentEdit(edit))
            store.applyPageWork(store.pageWorkResult(for: request), for: request)
        }
        let photo = try XCTUnwrap(try read().first)
        XCTAssertEqual(photo.bbox.x, 72, accuracy: 0.5)
        XCTAssertEqual(photo.bbox.width, 100, accuracy: 0.5)

        try perform(.moveImage(photo, to: PdfRect(x: 172, y: 500, width: 100, height: 50)))
        XCTAssertEqual(try XCTUnwrap(try read().first).bbox.x, 172, accuracy: 0.5)
        XCTAssertTrue(store.canUndo)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        let back = try XCTUnwrap(try read().first)
        XCTAssertEqual(back.bbox.x, 72, accuracy: 0.5)

        try perform(.resizeImage(back, to: PdfRect(x: 72, y: 600, width: 200, height: 100)))
        XCTAssertEqual(try XCTUnwrap(try read().first).bbox.width, 200, accuracy: 0.5)

        store.undo()
        if let refresh = store.beginPageRefresh() { store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh) }
        try perform(.removeImage(try XCTUnwrap(try read().first)))
        XCTAssertEqual(try read(), [])
    } }
}


extension UniFfiContentEditIntegrationTests {
    /// A 4 × 2 blue PNG, made for the run.
    static func bluePng() throws -> Data {
        let rep = try XCTUnwrap(NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: 4, pixelsHigh: 2, bitsPerSample: 8, samplesPerPixel: 3,
            hasAlpha: false, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
        ))
        for x in 0..<4 { for y in 0..<2 { rep.setColor(.blue, atX: x, y: y) } }
        return try XCTUnwrap(rep.representation(using: .png, properties: [:]))
    }

    /// A new image is added against the real core, and reads back as one of
    /// the page's own images with its top-left corner at the click.
    func testAnImageIsAddedWhereClicked() throws {
        let png = try Self.bluePng()
        try offMain {
            let store = ViewerStore(client: UniFfiPdfCoreClient())
            store.open(bytes: Self.onePhotoPdf())
            store.setContentEditMode(true)
            func read() throws -> [ContentImage] {
                let request = try XCTUnwrap(store.beginPageContent(page: 0))
                store.applyPageContent(store.pageContentResult(for: request), for: request)
                return try XCTUnwrap(store.pageContent[0]).images
            }

            let insert = try XCTUnwrap(store.beginImageInsert(page: 0, at: PdfPoint(x: 300, y: 300), bytes: png))
            store.applyPageWork(store.pageWorkResult(for: insert), for: insert)
            let images = try read()
            XCTAssertEqual(images.count, 2)
            let added = try XCTUnwrap(images.first { $0.resourceXObjectName?.hasPrefix("VitelaImage") == true })
            XCTAssertEqual(added.bbox.x, 300, accuracy: 0.5)
            XCTAssertEqual(added.bbox.y + added.bbox.height, 300, accuracy: 0.5, "its top-left corner is at the click")
            XCTAssertEqual(added.bbox.width / added.bbox.height, 2, accuracy: 0.01, "the PNG's own proportions")

            XCTAssertTrue(store.canUndo)
            // A just-inserted image has a pending edit, and the unfiltered
            // original cannot be read back losslessly: the core refuses both.
            let document = try XCTUnwrap(store.document)
            XCTAssertEqual(store.client.imageReplacementRefusal(document: document, image: added), ContentEditing.replaceRefused)
            let raw = try XCTUnwrap(images.first { $0.resourceXObjectName == "Im1" })
            XCTAssertEqual(store.client.imageReplacementRefusal(document: document, image: raw), ContentEditing.replaceRefused)
        }
    }

    /// A Flate-encoded image the core can read back has its picture replaced
    /// against the real core, as one undoable step.
    func testAFlateImageIsReplaced() throws {
        let png = try Self.bluePng()
        try offMain {
            let store = ViewerStore(client: UniFfiPdfCoreClient())
            store.open(bytes: Self.onePhotoPdf(flate: true))
            XCTAssertEqual(store.state, .loaded)
            store.setContentEditMode(true)
            let request = try XCTUnwrap(store.beginPageContent(page: 0))
            store.applyPageContent(store.pageContentResult(for: request), for: request)
            let photo = try XCTUnwrap(store.pageContent[0]?.images.first)
            XCTAssertNil(store.client.imageReplacementRefusal(document: try XCTUnwrap(store.document), image: photo))

            let replace = try XCTUnwrap(store.beginContentEdit(.replaceImage(photo, with: png)))
            store.applyPageWork(store.pageWorkResult(for: replace), for: replace)

            XCTAssertEqual(store.editStatus, "Image replaced. Changes are pending save.")
            XCTAssertTrue(store.canUndo)
        }
    }
}
