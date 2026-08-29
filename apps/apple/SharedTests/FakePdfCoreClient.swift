// Shared test double for `PdfCoreClient`/`PageCharacters`, used by
// ViewerStoreTests.swift, ViewerStoreSearchTests.swift and
// ViewerStoreSelectionTests.swift.
import Foundation
@testable import Vitela

final class FakePdfCoreClient: PdfCoreClient {
    let pages: [PageDimensions]
    var openedBytes: [Data] = []
    var openedPasswords: [String?] = []
    var requestedDpi: [Int] = []
    var openError: ViewerFailure?
    var searchResults: [String: [SearchMatch]] = [:]
    var searchError: Error?
    var searchedQueries: [String] = []
    var pageCharactersByPage: [Int: PageCharacters] = [:]
    var pageCharactersError: Error?
    var pageCharactersRequests: [Int] = []

    init(pages: [PageDimensions]) {
        self.pages = pages
    }

    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        openedBytes.append(bytes)
        openedPasswords.append(password)
        if let openError {
            throw openError
        }
        return FakeCoreDocument(pages: pages)
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        requestedDpi.append(dpi)
        return RenderedPage.placeholder
    }

    func search(document: any PdfDocument, query: String) throws -> [SearchMatch] {
        searchedQueries.append(query)
        if let searchError {
            throw searchError
        }
        return searchResults[query] ?? []
    }

    func pageCharacters(document: any PdfDocument, page: Int) throws -> any PageCharacters {
        pageCharactersRequests.append(page)
        if let pageCharactersError {
            throw pageCharactersError
        }
        guard let characters = pageCharactersByPage[page] else {
            throw TextQueryFailure.failed("no fake characters configured for page \(page)")
        }
        return characters
    }
}

struct FakeCoreDocument: PdfDocument {
    let pages: [PageDimensions]
}

/// A `PageCharacters` double whose caret/text/rect answers are supplied by
/// closures, so each test only has to describe the behavior it cares about.
final class FakePageCharacters: PageCharacters {
    private let caretAtHandler: (Double, Double) -> Int?
    private let textInHandler: (Int, Int) -> String
    private let rectsInHandler: (Int, Int) -> [TextRect]

    init(
        caretAt: @escaping (Double, Double) -> Int? = { _, _ in nil },
        textIn: @escaping (Int, Int) -> String = { _, _ in "" },
        rectsIn: @escaping (Int, Int) -> [TextRect] = { _, _ in [] }
    ) {
        caretAtHandler = caretAt
        textInHandler = textIn
        rectsInHandler = rectsIn
    }

    func caretAt(xPt: Double, yPt: Double) -> Int? { caretAtHandler(xPt, yPt) }
    func textIn(anchor: Int, focus: Int) -> String { textInHandler(anchor, focus) }
    func rectsIn(anchor: Int, focus: Int) -> [TextRect] { rectsInHandler(anchor, focus) }
}
