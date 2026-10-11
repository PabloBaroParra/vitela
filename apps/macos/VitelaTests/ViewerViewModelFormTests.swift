import XCTest
@testable import Vitela

final class ViewerViewModelFormTests: XCTestCase {
    func testAFillRunsOnTheQueue() {
        let client = FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
        client.fields = [FormField(id: 7, page: 0, name: "Agree", rect: PdfRect(x: 0, y: 0, width: 10, height: 10), kind: .checkbox, value: .checked(false))]
        let model = ViewerViewModel(store: ViewerStore(client: client))
        model.store.open(bytes: Data([1]))

        model.fillField(7, value: .checked(true))

        waitUntil({ model.store.formFields.first?.value == .checked(true) }, "the fill never landed")
        XCTAssertFalse(model.store.pageWorkInFlight)
    }
}
