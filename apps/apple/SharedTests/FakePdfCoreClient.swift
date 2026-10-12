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
    // Editing: a tiny stand-in for the core's EditLog — a list of
    // (Info, page layout) snapshots plus a cursor, which is all undo/redo of
    // metadata and page edits needs.
    struct Snapshot {
        var info: DocumentInfo
        var pages: [PageDimensions]
    }

    var initialInfo = DocumentInfo()
    var editingAllowed = true
    var setInfoError: Error?
    var history: [Snapshot] = []
    var historyCursor = 0
    /// The Info entries of `history`, for tests about metadata only.
    var infoHistory: [DocumentInfo] {
        get { history.map(\.info) }
        set { history = newValue.map { Snapshot(info: $0, pages: pages) } }
    }
    // Page edits.
    var pageEdits: [PageEdit] = []
    var pageEditError: Error?
    var refreshPreviewError: Error?
    var refreshPreviewCount = 0
    /// What `render` draws: the layout as of the last `refreshPreview`, as in
    /// the core, where `pageDimensions` reads the render-side document.
    var previewPages: [PageDimensions]?
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
    // Signing.
    var signingRefusalText: String?
    var certificatePassword = "secret"
    var signingIdentities = [SigningIdentity(id: "id-1", displayName: "Test Signer")]
    var unlockAttempts: [String] = []
    var signedAs: [String] = []
    var signError: Error?
    var signedBytes = Data("%PDF-signed".utf8)
    var reopenedSigned: [Data] = []
    // Protecting.
    var protectInvalidatesSignatures = false
    var protectedBytes = Data("%PDF-protected".utf8)
    var protectError: Error?
    var protectCalls: [(open: String, permissions: String, acknowledged: Bool)] = []
    var openedWithPasswords: [(user: String, owner: String)] = []
    var blankDocumentsCreated = 0

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
        historyCursor == 0 ? initialInfo : history[historyCursor - 1].info
    }

    /// The model's page layout, pending edits included.
    var currentPages: [PageDimensions] {
        historyCursor == 0 ? pages : history[historyCursor - 1].pages
    }

    private func record(_ snapshot: Snapshot) {
        history = Array(history.prefix(historyCursor)) + [snapshot]
        historyCursor = history.count
    }

    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws {
        if let setInfoError { throw setInfoError }
        record(Snapshot(info: info, pages: currentPages))
    }

    func applyPageEdit(document: any PdfDocument, edit: PageEdit) throws {
        if let pageEditError { throw pageEditError }
        pageEdits.append(edit)
        var layout = currentPages
        switch edit {
        case let .rotate(page, degrees):
            if (degrees / 90) % 2 != 0 {
                layout[page] = PageDimensions(width: layout[page].height, height: layout[page].width)
            }
        case let .remove(page):
            layout.remove(at: page)
        case let .insertBlank(at, orientation):
            let a4 = orientation == .portrait
                ? PageDimensions(width: 595, height: 842)
                : PageDimensions(width: 842, height: 595)
            layout.insert(a4, at: at)
        case let .move(from, to):
            layout.insert(layout.remove(at: from), at: to)
        }
        record(Snapshot(info: try documentInfo(document: document), pages: layout))
    }

    func refreshPreview(document: any PdfDocument) throws {
        refreshPreviewCount += 1
        if let refreshPreviewError { throw refreshPreviewError }
        previewPages = currentPages
    }

    func pageDimensions(document: any PdfDocument) -> [PageDimensions] {
        previewPages ?? pages
    }

    func contentEditingAllowed(document: any PdfDocument) -> Bool { editingAllowed }

    func undo(document: any PdfDocument) -> Bool {
        guard historyCursor > 0 else { return false }
        historyCursor -= 1
        return true
    }

    func redo(document: any PdfDocument) -> Bool {
        guard historyCursor < history.count else { return false }
        historyCursor += 1
        return true
    }

    func canUndo(document: any PdfDocument) -> Bool { historyCursor > 0 }
    func canRedo(document: any PdfDocument) -> Bool { historyCursor < history.count }

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

    func open(bytes: Data, userPassword: String, ownerPassword: String) throws -> any PdfDocument {
        openedWithPasswords.append((userPassword, ownerPassword))
        return try open(bytes: bytes, password: ownerPassword)
    }

    func createBlankDocument() throws -> any PdfDocument {
        blankDocumentsCreated += 1
        return FakeCoreDocument(pages: [PageDimensions(width: 595, height: 842)])
    }

    func protectionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool { protectInvalidatesSignatures }

    func protect(
        document: any PdfDocument, openPassword: String, permissionsPassword: String, acknowledgingSignatureLoss: Bool
    ) throws -> Data {
        protectCalls.append((openPassword, permissionsPassword, acknowledgingSignatureLoss))
        if let protectError { throw protectError }
        return protectedBytes
    }
}

struct FakeSigningSource: SigningSource {
    var identities: [SigningIdentity]
}

extension FakePdfCoreClient {
    func signingRefusal(document: any PdfDocument) -> String? { signingRefusalText }

    func openSigningCertificate(bytes: Data, password: String) throws -> any SigningSource {
        unlockAttempts.append(password)
        guard password == certificatePassword else { throw SigningSourceFailure.wrongPassword }
        return FakeSigningSource(identities: signingIdentities)
    }

    func sign(document: any PdfDocument, source: any SigningSource, identityId: String) throws -> Data {
        signedAs.append(identityId)
        if let signError { throw signError }
        return signedBytes
    }

    func reopenSigned(document: any PdfDocument, bytes: Data) throws -> any PdfDocument {
        reopenedSigned.append(bytes)
        return FakeCoreDocument(pages: pages)
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
