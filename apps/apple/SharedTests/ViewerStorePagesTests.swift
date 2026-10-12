import XCTest
@testable import Vitela

final class ViewerStorePagesTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)
    private let a5 = PageDimensions(width: 420, height: 595)

    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    /// Runs a request the way the shell does, minus the queue hop.
    private func run(_ store: ViewerStore, _ request: ViewerStore.PageWorkRequest?) throws {
        let request = try XCTUnwrap(request)
        store.applyPageWork(store.pageWorkResult(for: request), for: request)
    }

    func testRotatingAPageRelaysTheDocumentOutFromTheRefreshedPreview() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        let generation = store.generation

        try run(store, store.beginPageEdit(.rotate(page: 1, degrees: 90)))

        XCTAssertEqual(client.pageEdits, [.rotate(page: 1, degrees: 90)])
        XCTAssertEqual(client.refreshPreviewCount, 1, "the render side must be re-derived before the sizes are read")
        XCTAssertEqual(store.pageSlots.map(\.dimensions), [letter, PageDimensions(width: 595, height: 420)])
        XCTAssertGreaterThan(store.generation, generation, "renders of the old layout must be dropped")
        XCTAssertEqual(store.editStatus, "Page rotated. Changes are pending save.")
        XCTAssertTrue(store.hasUnsavedChanges)
        XCTAssertTrue(store.canUndo)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    func testARelayoutForgetsEverythingAddressedByPagePosition() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        let staleRender = try XCTUnwrap(store.beginRender(page: 0))
        store.selection = TextSelection(page: 1, anchor: 0, focus: 3)
        store.search = SearchSession(query: "x", matches: [], currentIndex: 0)

        try run(store, store.beginPageEdit(.rotate(page: 0, degrees: -90)))
        store.applyRender(.success(RenderedPage.placeholder), for: staleRender)

        XCTAssertNil(store.selection)
        XCTAssertNil(store.search)
        XCTAssertNil(store.pageSlots[0].image, "a render of the old layout must not land on the new one")
        XCTAssertEqual(store.pageSlots[0].status, .idle)
    }

    func testOnlyOnePageEditRunsAtATimeAndHistoryWaitsForIt() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        store.setDocumentProperty(.title, to: "Before")

        let first = try XCTUnwrap(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        XCTAssertNil(store.beginPageEdit(.rotate(page: 1, degrees: 90)))
        XCTAssertFalse(store.canEditPages)
        store.undo()
        XCTAssertEqual(client.historyCursor, 1, "an undo under a page edit in flight would undo the step before it")

        store.applyPageWork(store.pageWorkResult(for: first), for: first)
        XCTAssertTrue(store.canEditPages)
    }

    func testARefusedEditReportsTheCoreReasonAndChangesNothing() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        client.pageEditError = EditFailure.notPermitted("This document does not allow its pages to be rearranged, rotated or removed.")
        let store = openStore(client)
        let generation = store.generation

        try run(store, store.beginPageEdit(.rotate(page: 0, degrees: 90)))

        XCTAssertEqual(store.editStatus, "This document does not allow its pages to be rearranged, rotated or removed.")
        XCTAssertEqual(store.generation, generation)
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertFalse(store.pagesEdited)
        XCTAssertEqual(client.refreshPreviewCount, 0)
    }

    /// The edit is in the history even though the preview could not be
    /// rebuilt: it must count as unsaved and stay undoable.
    func testAFailedRefreshAfterAnAppliedEditStillRecordsTheStep() throws {
        let client = FakePdfCoreClient(pages: [letter])
        client.refreshPreviewError = EditFailure.failed("Render failed.")
        let store = openStore(client)

        try run(store, store.beginPageEdit(.rotate(page: 0, degrees: 90)))

        XCTAssertEqual(store.editStatus, "Render failed.")
        XCTAssertTrue(store.hasUnsavedChanges)
        XCTAssertTrue(store.canUndo)
        XCTAssertTrue(store.pagesEdited)
    }

    func testInvalidEditsAreRefusedBeforeReachingTheCore() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)

        XCTAssertNil(store.beginPageEdit(.rotate(page: 1, degrees: 90)))
        XCTAssertNil(store.beginPageEdit(.rotate(page: 0, degrees: 45)))
        XCTAssertNil(store.beginPageEdit(.rotate(page: 0, degrees: 360)))
        XCTAssertNil(store.beginPageEdit(.remove(page: 0)))
        XCTAssertEqual(store.editStatus, "A document needs at least one page.")
        XCTAssertNil(store.beginPageEdit(.insertBlank(at: 2, orientation: .portrait)))
        XCTAssertNil(store.beginPageEdit(.move(from: 0, to: 0)))
        XCTAssertTrue(client.pageEdits.isEmpty)
        XCTAssertFalse(store.pageWorkInFlight)
    }

    func testNoDocumentMeansNoPageEdit() {
        let store = ViewerStore(client: FakePdfCoreClient(pages: [letter]))

        XCTAssertNil(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        XCTAssertFalse(store.canEditPages)
    }

    func testUndoAfterAPageEditOwesTheViewerARelayout() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        XCTAssertNil(store.beginPageRefresh(), "a session that never edited its pages needs no relayout")
        try run(store, store.beginPageEdit(.rotate(page: 0, degrees: 90)))

        store.undo()
        try run(store, store.beginPageRefresh())

        XCTAssertEqual(store.pageSlots.map(\.dimensions), [letter, a5])
        XCTAssertEqual(store.editStatus, "Undone.", "the relayout must not overwrite what the undo reported")

        store.redo()
        try run(store, store.beginPageRefresh())
        XCTAssertEqual(store.pageSlots[0].dimensions, PageDimensions(width: 792, height: 612))
    }

    func testAPageEditFinishingAfterAnotherDocumentOpenedIsDropped() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        let request = try XCTUnwrap(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        let outcome = store.pageWorkResult(for: request)

        store.open(bytes: Data([2]))
        store.applyPageWork(outcome, for: request)

        XCTAssertEqual(store.editStatus, "")
        XCTAssertFalse(store.pageWorkInFlight)
        XCTAssertFalse(store.pagesEdited)
    }

    // MARK: - Thumbnails

    func testThumbnailsFitTheLongerSideIntoTheCardBox() {
        XCTAssertEqual(ViewerStore.thumbnailDpi(for: letter, scale: 1), 14)
        XCTAssertEqual(ViewerStore.thumbnailDpi(for: letter, scale: 2), 28)
        XCTAssertEqual(ViewerStore.thumbnailDpi(for: PageDimensions(width: 1, height: 1), scale: 2), 300)
        XCTAssertEqual(ViewerStore.thumbnailDpi(for: PageDimensions(width: 14400, height: 14400), scale: 1), 8)
    }

    func testAThumbnailIsRenderedOnceAndDoesNotTouchTheReaderSlots() throws {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)

        let request = try XCTUnwrap(store.beginThumbnail(page: 0, scale: 2))
        XCTAssertNil(store.beginThumbnail(page: 0, scale: 2), "a thumbnail on its way must not be asked for twice")
        store.applyThumbnail(store.renderResult(for: request), for: request)

        XCTAssertNotNil(store.thumbnails[0])
        XCTAssertNil(store.beginThumbnail(page: 0, scale: 2))
        XCTAssertEqual(store.pageSlots[0].status, .idle)
        XCTAssertEqual(client.requestedDpi, [28])
    }

    func testARelayoutClearsThumbnailsAndDropsLateOnes() throws {
        let client = FakePdfCoreClient(pages: [letter, a5])
        let store = openStore(client)
        let done = try XCTUnwrap(store.beginThumbnail(page: 0, scale: 1))
        store.applyThumbnail(store.renderResult(for: done), for: done)
        let late = try XCTUnwrap(store.beginThumbnail(page: 1, scale: 1))

        try run(store, store.beginPageEdit(.rotate(page: 1, degrees: 90)))
        store.applyThumbnail(.success(RenderedPage.placeholder), for: late)

        XCTAssertTrue(store.thumbnails.isEmpty)
        XCTAssertNotNil(store.beginThumbnail(page: 1, scale: 1), "the turned page must be drawn again")
    }
}

extension ViewerStorePagesTests {
    func testARelayoutKeepsTheCurrentPageInsideTheDocument() throws {
        let client = FakePdfCoreClient(pages: Array(repeating: PageDimensions(width: 612, height: 792), count: 3))
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        store.goToPage(2)

        let request = try XCTUnwrap(store.beginPageEdit(.rotate(page: 0, degrees: 90)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertNil(store.requestedPage, "a request names a position in the old layout")
        XCTAssertEqual(store.currentPage, 2)
        XCTAssertEqual(store.pagePositionText, "Page 3 of 3")
    }
}

extension ViewerStorePagesTests {
    func testDeletingAPageRemovesItsSlotAndIsOneUndoStep() throws {
        let letter = PageDimensions(width: 612, height: 792)
        let a5 = PageDimensions(width: 420, height: 595)
        let client = FakePdfCoreClient(pages: [letter, a5, letter])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))

        let request = try XCTUnwrap(store.beginPageEdit(.remove(page: 1)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertEqual(store.pageSlots.map(\.dimensions), [letter, letter])
        XCTAssertEqual(store.pageSlots.map(\.index), [0, 1], "positions after the deleted page move up")
        XCTAssertEqual(store.editStatus, "Page deleted. Changes are pending save.")

        store.undo()
        let refresh = try XCTUnwrap(store.beginPageRefresh())
        store.applyPageWork(store.pageWorkResult(for: refresh), for: refresh)
        XCTAssertEqual(store.pageSlots.map(\.dimensions), [letter, a5, letter])
    }

    func testTheLastRemainingPageCannotBeDeleted() throws {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792), PageDimensions(width: 612, height: 792)])
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        let request = try XCTUnwrap(store.beginPageEdit(.remove(page: 0)))
        store.applyPageWork(store.pageWorkResult(for: request), for: request)

        XCTAssertNil(store.beginPageEdit(.remove(page: 0)))
        XCTAssertEqual(store.editStatus, "A document needs at least one page.")
        XCTAssertEqual(client.pageEdits, [.remove(page: 0)])
    }
}
