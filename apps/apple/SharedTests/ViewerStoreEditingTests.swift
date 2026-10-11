import XCTest
@testable import Vitela

final class ViewerStoreEditingTests: XCTestCase {
    private let letter = PageDimensions(width: 612, height: 792)

    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    func testOpeningReadsPropertiesAndStartsClean() {
        let client = FakePdfCoreClient(pages: [letter])
        client.initialInfo = DocumentInfo(title: "Report")
        let store = openStore(client)

        XCTAssertEqual(store.documentInfo?.title, "Report")
        XCTAssertTrue(store.editingAllowed)
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertFalse(store.canUndo)
    }

    func testSettingAPropertyIsAnUndoableUnsavedEdit() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)

        store.setDocumentProperty(.author, to: "Ada")

        XCTAssertEqual(store.documentInfo?.author, "Ada")
        XCTAssertEqual(client.infoHistory.last?.author, "Ada")
        XCTAssertTrue(store.hasUnsavedChanges)
        XCTAssertTrue(store.canUndo)
        XCTAssertEqual(store.editStatus, "Document properties updated. Changes are pending save.")
    }

    func testAnEmptyValueRemovesThePropertyAndAnUnchangedOneAddsNoStep() {
        let client = FakePdfCoreClient(pages: [letter])
        client.initialInfo = DocumentInfo(title: "Report")
        let store = openStore(client)

        store.setDocumentProperty(.title, to: "Report")
        XCTAssertTrue(client.infoHistory.isEmpty, "retyping the same value must not add an undo step")

        store.setDocumentProperty(.title, to: "")
        XCTAssertNil(store.documentInfo?.title)
    }

    func testUndoAndRedoRestorePropertiesAndHistoryFlags() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)
        store.setDocumentProperty(.subject, to: "Q3")

        store.undo()
        XCTAssertNil(store.documentInfo?.subject)
        XCTAssertFalse(store.canUndo)
        XCTAssertTrue(store.canRedo)

        store.redo()
        XCTAssertEqual(store.documentInfo?.subject, "Q3")
        XCTAssertTrue(store.canUndo)
        XCTAssertFalse(store.canRedo)
    }

    func testAValidDateIsStoredWithThePreviousOffset() {
        let client = FakePdfCoreClient(pages: [letter])
        let prior = MetadataDate(year: 2020, month: 1, day: 2, hour: 3, minute: 4, second: 5, offset: .plus(hours: 2, minutes: 0))
        client.initialInfo = DocumentInfo(creationDate: prior)
        let store = openStore(client)

        XCTAssertTrue(store.setDocumentDate(.created, to: "2024-12-31 23:59"))

        XCTAssertEqual(
            store.documentInfo?.creationDate,
            MetadataDate(year: 2024, month: 12, day: 31, hour: 23, minute: 59, second: 0, offset: .plus(hours: 2, minutes: 0))
        )
    }

    func testAnInvalidDateIsRejectedWithoutTouchingTheDocument() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)

        XCTAssertFalse(store.setDocumentDate(.modified, to: "2024-13-01"))

        XCTAssertTrue(client.infoHistory.isEmpty)
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertEqual(store.editStatus, "Invalid date, reverted: month 13 is out of range (1-12)")
    }

    func testADocumentThatForbidsChangesRefusesPropertyEdits() {
        let client = FakePdfCoreClient(pages: [letter])
        client.editingAllowed = false
        let store = openStore(client)

        store.setDocumentProperty(.title, to: "New")

        XCTAssertTrue(client.infoHistory.isEmpty)
        XCTAssertEqual(store.editStatus, "This document does not permit metadata changes.")
    }

    func testACoreRefusalIsReportedAndLeavesTheShownValue() {
        let client = FakePdfCoreClient(pages: [letter])
        client.setInfoError = EditFailure.notPermitted("Content editing is not permitted.")
        let store = openStore(client)

        store.setDocumentProperty(.title, to: "New")

        XCTAssertNil(store.documentInfo?.title)
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertEqual(store.editStatus, "Content editing is not permitted.")
    }

    func testASaveMarksTheRevisionItWasComputedFromAsSaved() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)
        store.setDocumentProperty(.title, to: "One")
        let request = try! XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        // An edit lands while the save is still computing its bytes.
        store.setDocumentProperty(.title, to: "Two")

        XCTAssertEqual(try store.saveResult(for: request).get(), client.savedBytes)
        store.applySaved(request)

        XCTAssertTrue(store.hasUnsavedChanges, "the edit made during the save is not in the saved file")
    }

    func testASaveThatWouldBreakASignatureNeedsAcknowledgement() {
        let client = FakePdfCoreClient(pages: [letter])
        client.invalidatesSignatures = true
        let store = openStore(client)

        let probe = try! XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: false))
        XCTAssertEqual(try store.saveWillInvalidateSignatures(for: probe).get(), true)
        XCTAssertEqual(store.saveResult(for: probe), .failure(.signaturesWouldBeInvalidated))

        let acknowledged = try! XCTUnwrap(store.beginSave(acknowledgingSignatureLoss: true))
        XCTAssertEqual(try store.saveResult(for: acknowledged).get(), client.savedBytes)
    }

    func testOpeningAnotherDocumentForgetsTheEditHistory() {
        let client = FakePdfCoreClient(pages: [letter])
        let store = openStore(client)
        store.setDocumentProperty(.title, to: "Edited")

        client.infoHistory = []
        client.historyCursor = 0
        store.open(bytes: Data([2]))

        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertFalse(store.canUndo)
        XCTAssertEqual(store.editStatus, "")
    }
}

final class MetadataDateTextTests: XCTestCase {
    func testFormatsAndParsesTheSharedDesktopForm() throws {
        let date = MetadataDate(year: 2024, month: 3, day: 9, hour: 7, minute: 5, second: 1, offset: .utc)
        XCTAssertEqual(MetadataDateText.format(date), "2024-03-09 07:05:01")
        XCTAssertEqual(try MetadataDateText.parse("2024-03-09 07:05:01", offset: .utc), date)
        XCTAssertEqual(MetadataDateText.format(nil), "")
    }

    func testTimeIsOptionalAndSecondsDefaultToZero() throws {
        XCTAssertEqual(try MetadataDateText.parse("  2024-03-09  ", offset: .utc)?.hour, 0)
        XCTAssertEqual(try MetadataDateText.parse("2024-03-09 10:30", offset: .utc)?.second, 0)
    }

    func testBlankClearsTheDate() throws {
        XCTAssertNil(try MetadataDateText.parse("   ", offset: .utc))
    }

    func testRejectsMalformedAndOutOfRangeInput() {
        XCTAssertThrowsError(try MetadataDateText.parse("2024/03/09", offset: .utc))
        XCTAssertThrowsError(try MetadataDateText.parse("2024-03-09 10", offset: .utc))
        XCTAssertThrowsError(try MetadataDateText.parse("2024-03-32", offset: .utc))
        XCTAssertThrowsError(try MetadataDateText.parse("2024-03-09 24:00", offset: .utc))
        XCTAssertThrowsError(try MetadataDateText.parse("2024-0x-09", offset: .utc))
    }
}
