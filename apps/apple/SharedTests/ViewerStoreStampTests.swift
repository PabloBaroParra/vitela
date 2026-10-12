import XCTest
@testable import Vitela

final class ViewerStoreStampTests: XCTestCase {
    private let png = Data([137, 80, 78, 71, 13, 10, 26, 10, 0, 0])

    private func armedStore() -> (FakePdfCoreClient, ViewerStore) {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.armAnnotationTool(.stamp)
        return (client, store)
    }

    func testOnlyPngAndJpegSignaturesAreStamps() {
        XCTAssertTrue(ViewerStore.isSupportedStampImage(png))
        XCTAssertTrue(ViewerStore.isSupportedStampImage(Data([0xFF, 0xD8, 0xFF, 0xE0])))
        XCTAssertFalse(ViewerStore.isSupportedStampImage(Data("GIF89a".utf8)))
        XCTAssertFalse(ViewerStore.isSupportedStampImage(Data()))
    }

    func testADraggedStampFillsItsRectAndLandsSelected() {
        let (client, store) = armedStore()
        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 500)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 300, y: 400))
        XCTAssertEqual(store.pendingStamp, PendingStamp(page: 0, rect: PdfRect(x: 100, y: 400, width: 200, height: 100), anchor: PdfPoint(x: 100, y: 500)))

        store.insertPendingStamp(imageBytes: png)

        XCTAssertEqual(client.stampInserts.map(\.rect), [PdfRect(x: 100, y: 400, width: 200, height: 100)])
        XCTAssertEqual(store.selectedAnnotation?.kind, .stamp)
        XCTAssertEqual(store.stampImages[store.selectedAnnotationId!], png, "kept for the preview")
        XCTAssertEqual(store.editStatus, "Image stamp added. Changes are pending save.")
        XCTAssertTrue(store.canUndo)
    }

    func testAClickLetsTheImageDecideItsSize() {
        let (client, store) = armedStore()
        XCTAssertTrue(store.beginAnnotationDraft(page: 0, at: PdfPoint(x: 100, y: 500)))
        store.endAnnotationDraft(page: 0, at: PdfPoint(x: 101, y: 500))

        store.insertPendingStamp(imageBytes: png)

        XCTAssertEqual(client.stampInserts.map(\.rect), [PdfRect(x: 100, y: 450, width: 100, height: 50)])
    }

    func testAnUnsupportedImageIsReportedAndNothingInserted() {
        let (client, store) = armedStore()
        store.beginStampPlacement(page: 0, origin: PdfPoint(x: 1, y: 1), release: PdfPoint(x: 1, y: 1))

        store.insertPendingStamp(imageBytes: Data("GIF89a".utf8))

        XCTAssertTrue(client.stampInserts.isEmpty)
        XCTAssertEqual(store.editStatus, "The image is not a supported PNG or JPEG.")
    }

    func testCancellingThePickerSaysSo() {
        let (_, store) = armedStore()
        store.beginStampPlacement(page: 0, origin: PdfPoint(x: 1, y: 1), release: PdfPoint(x: 1, y: 1))

        store.cancelPendingStamp()

        XCTAssertNil(store.pendingStamp)
        XCTAssertEqual(store.editStatus, "Stamp placement canceled.")
    }
}
