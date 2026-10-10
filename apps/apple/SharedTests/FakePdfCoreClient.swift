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
    // Editing: a tiny stand-in for the core's EditLog — a list of Info
    // snapshots plus a cursor, which is all undo/redo of metadata needs.
    var initialInfo = DocumentInfo()
    var editingAllowed = true
    var setInfoError: Error?
    var infoHistory: [DocumentInfo] = []
    var historyCursor = 0
    var invalidatesSignatures = false
    var savedBytes = Data("%PDF-fake".utf8)
    var saveError: Error?
    var saveAcknowledgements: [Bool] = []

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

    func documentInfo(document: any PdfDocument) throws -> DocumentInfo {
        historyCursor == 0 ? initialInfo : infoHistory[historyCursor - 1]
    }

    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws {
        if let setInfoError { throw setInfoError }
        infoHistory = Array(infoHistory.prefix(historyCursor)) + [info]
        historyCursor = infoHistory.count
    }

    func contentEditingAllowed(document: any PdfDocument) -> Bool { editingAllowed }

    func undo(document: any PdfDocument) -> Bool {
        guard historyCursor > 0 else { return false }
        historyCursor -= 1
        return true
    }

    func redo(document: any PdfDocument) -> Bool {
        guard historyCursor < infoHistory.count else { return false }
        historyCursor += 1
        return true
    }

    func canUndo(document: any PdfDocument) -> Bool { historyCursor > 0 }
    func canRedo(document: any PdfDocument) -> Bool { historyCursor < infoHistory.count }

    func saveWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { invalidatesSignatures }

    func save(document: any PdfDocument, acknowledgingSignatureLoss: Bool) throws -> Data {
        saveAcknowledgements.append(acknowledgingSignatureLoss)
        if let saveError { throw saveError }
        if invalidatesSignatures && !acknowledgingSignatureLoss { throw EditFailure.signaturesWouldBeInvalidated }
        return savedBytes
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
