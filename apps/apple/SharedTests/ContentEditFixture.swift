import Foundation
@testable import Vitela

/// What the Edit Content store tests share: a line, a one-page document, and
/// reading a page the way the shell does, synchronously.
enum ContentEditFixture {
    static func run(_ id: UInt64, _ rect: PdfRect, text: String = "Hello", kind: ContentFontKind = .standard14, page: Int = 0) -> ContentTextRun {
        ContentTextRun(id: id, page: page, bbox: rect, resourceFontName: "F1", fontKind: kind, text: text)
    }

    static func client() -> FakePdfCoreClient {
        FakePdfCoreClient(pages: [PageDimensions(width: 612, height: 792)])
    }

    static func opened(_ client: FakePdfCoreClient = client()) -> (FakePdfCoreClient, ViewerStore) {
        let store = ViewerStore(client: client)
        store.open(bytes: Data([1]))
        return (client, store)
    }

    static func read(_ store: ViewerStore, page: Int = 0) {
        guard let request = store.beginPageContent(page: page) else { return }
        store.applyPageContent(store.pageContentResult(for: request), for: request)
    }
}
