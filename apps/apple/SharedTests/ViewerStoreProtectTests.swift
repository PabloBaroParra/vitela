import XCTest
@testable import Vitela

final class ViewerStoreProtectTests: XCTestCase {
    private func openStore(_ client: FakePdfCoreClient) -> ViewerStore {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return store
    }

    private func client() -> FakePdfCoreClient { FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)]) }

    func testPasswordsAreCheckedInWindowsOrder() {
        XCTAssertEqual(
            ViewerStore.protectionPasswordProblem(open: "", permissions: ""),
            "Enter the password the document will ask for when it is opened."
        )
        XCTAssertEqual(ViewerStore.protectionPasswordProblem(open: "a", permissions: ""), "Enter the permissions password.")
        XCTAssertEqual(
            ViewerStore.protectionPasswordProblem(open: "same", permissions: "same"),
            "The two passwords must be different. If they were the same, everyone who can open the document could also change what it permits."
        )
        XCTAssertNil(ViewerStore.protectionPasswordProblem(open: "a", permissions: "b"))
    }

    func testADocumentThatForbidsChangesCannotBeProtected() {
        let client = client()
        client.editingAllowed = false
        let store = openStore(client)

        XCTAssertEqual(store.protectionRefusal(), "This document does not permit changes to its protection.")
        XCTAssertNil(store.beginProtect(openPassword: "a", permissionsPassword: "b"))
    }

    func testTheProtectedFileIsReopenedWithBothPasswordsAndHistoryStartsOver() throws {
        let client = client()
        let store = openStore(client)
        store.setDocumentProperty(.title, to: "Pending")
        let request = try XCTUnwrap(store.beginProtect(openPassword: "open-pass", permissionsPassword: "owner-pass"))

        let bytes = try store.protectResult(for: request, acknowledgingSignatureLoss: false).get()
        XCTAssertTrue(store.reopenProtected(bytes, for: request))

        XCTAssertEqual(client.openedBytes.last, client.protectedBytes)
        XCTAssertEqual(client.openedWithPasswords.map(\.user), ["open-pass"])
        XCTAssertEqual(client.openedWithPasswords.map(\.owner), ["owner-pass"])
        XCTAssertFalse(store.hasUnsavedChanges)
        XCTAssertEqual(store.editStatus, "Protected PDF saved and reopened.")
    }

    func testAProtectionForADocumentNoLongerOnScreenIsNotReopened() throws {
        let client = client()
        let store = openStore(client)
        let request = try XCTUnwrap(store.beginProtect(openPassword: "a", permissionsPassword: "b"))
        store.open(bytes: Data([2]))

        XCTAssertFalse(store.reopenProtected(Data("%PDF-x".utf8), for: request))
        XCTAssertEqual(client.openedWithPasswords.count, 0)
    }
}
