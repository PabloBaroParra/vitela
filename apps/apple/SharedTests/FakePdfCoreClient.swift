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
    var requestedPages: [Int] = []
    var renderError: Error?
    var renderedImage = RenderedPage.placeholder
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
    // Extracting pages.
    var textExtractionPermitted = true
    var fullRewritePermitted = true
    var sourceIsSigned = false
    var extractedBytes = Data("%PDF-extract".utf8)
    var extractError: Error?
    var extractedPages: [[Int]] = []
    /// How many extractions succeed before `extractError` is thrown.
    var failExtractsAfter = 0
    // Exporting images.
    var exportedImages: [(page: Int, dpi: Int, format: ImageFormat)] = []
    var exportError: Error?
    var failExportsAfter = 0
    var oversizedPage: Int?
    // Compressing.
    var compressRefusalText: String?
    var compressInvalidatesSignatures = false
    var compression = CompressionResult(
        bytes: Data("%PDF-small".utf8), beforeBytes: 2_000_000, afterBytes: 1_005_000, savedBytes: 995_000,
        reduced: true, refusals: []
    )
    var compressError: Error?
    var compressCalls: [(preset: CompressPreset, acknowledged: Bool)] = []

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
        requestedPages.append(page)
        if let renderError { throw renderError }
        return renderedImage
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

extension FakePdfCoreClient {
    func textExtractionAllowed(document: any PdfDocument) -> Bool { textExtractionPermitted }
    func fullRewriteAllowed(document: any PdfDocument) -> Bool { fullRewritePermitted }
    func extractSourceIsSigned(document: any PdfDocument) -> Bool { sourceIsSigned }

    /// Enough of the core's grammar for the tests: `"1-3,7"`, one-based.
    func parsePageSelection(_ text: String, totalPages: Int) throws -> [Int] {
        var pages = Set<Int>()
        for part in text.split(separator: ",") {
            let bounds = part.split(separator: "-").map { Int($0.trimmingCharacters(in: .whitespaces)) }
            guard let first = bounds.first ?? nil, let last = bounds.last ?? nil, bounds.count <= 2,
                  first >= 1, first <= last, last <= totalPages
            else { throw EditFailure.failed("\"\(part)\" is not a page range in this document.") }
            pages.formUnion((first - 1)...(last - 1))
        }
        return pages.sorted()
    }

    /// Cuts after each typed page; parts are named `<stem>-<n>.pdf`.
    func planSplit(_ cuts: String, totalPages: Int, documentName: String) throws -> [SplitPart] {
        let after = try cuts.split(separator: ",").map { text -> Int in
            guard let page = Int(text.trimmingCharacters(in: .whitespaces)), (1..<totalPages).contains(page) else {
                throw EditFailure.failed("\"\(text)\" is not a page to split after.")
            }
            return page
        }
        let stem = (documentName as NSString).deletingPathExtension
        let bounds = [0] + after.sorted() + [totalPages]
        return zip(bounds, bounds.dropFirst()).enumerated().map { index, range in
            SplitPart(first: range.0, last: range.1 - 1, fileName: "\(stem)-\(index + 1).pdf")
        }
    }

    func exportPageImage(document: any PdfDocument, page: Int, dpi: Int, format: ImageFormat) throws -> Data {
        exportedImages.append((page, dpi, format))
        if let exportError, exportedImages.count > failExportsAfter { throw exportError }
        return Data("image-\(page)".utf8)
    }

    func firstPageTooLargeToExport(document: any PdfDocument, pages: [Int], dpi: Int) -> Int? {
        pages.first { $0 == oversizedPage }
    }

    func pageImageFileName(documentName: String, page: Int, totalPages: Int, format: ImageFormat) -> String {
        "\((documentName as NSString).deletingPathExtension)-\(page + 1).\(format == .png ? "png" : "jpg")"
    }

    func extractPages(document: any PdfDocument, pages: [Int]) throws -> Data {
        extractedPages.append(pages)
        if let extractError, extractedPages.count > failExtractsAfter { throw extractError }
        return extractedBytes
    }

    func compressionRefusal(document: any PdfDocument) -> String? { compressRefusalText }
    func compressionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { compressInvalidatesSignatures }

    func compress(document: any PdfDocument, preset: CompressPreset, acknowledgingSignatureLoss: Bool) throws -> CompressionResult {
        compressCalls.append((preset, acknowledgingSignatureLoss))
        if let compressError { throw compressError }
        return compression
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
