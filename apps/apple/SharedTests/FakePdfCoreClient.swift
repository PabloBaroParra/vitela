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
        var annotations: [Annotation] = []
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
    // Annotations.
    var annotationsPermitted = true
    var annotationEditError: Error?
    var annotationEdits: [AnnotationEdit] = []
    private var nextAnnotationId: UInt64 = 1
    var currentAnnotations: [Annotation] {
        historyCursor == 0 ? [] : history[historyCursor - 1].annotations
    }
    // Page edits.
    var pageEdits: [PageEdit] = []
    var pageEditError: Error?
    var refreshPreviewError: Error?
    var refreshPreviewCount = 0
    /// What `render` draws: the layout as of the last `refreshPreview`, as in
    /// the core, where `pageDimensions` reads the render-side document.
    var previewPages: [PageDimensions]?
    // Importing.
    var importRefusalText: String?
    /// Sources by their file bytes; a source with a password needs it typed.
    var importSources: [Data: FakeImportSource] = [:]
    var preparedPasswords: [String?] = []
    var importError: Error?
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

    func appendToHistory(pages: [PageDimensions]) {
        record(Snapshot(
            info: (try? documentInfo(document: FakeCoreDocument(pages: pages))) ?? initialInfo,
            pages: pages,
            annotations: currentAnnotations
        ))
    }

    private func record(_ snapshot: Snapshot) {
        history = Array(history.prefix(historyCursor)) + [snapshot]
        historyCursor = history.count
    }

    func setDocumentInfo(document: any PdfDocument, info: DocumentInfo) throws {
        if let setInfoError { throw setInfoError }
        record(Snapshot(info: info, pages: currentPages, annotations: currentAnnotations))
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
        record(Snapshot(info: try documentInfo(document: document), pages: layout, annotations: currentAnnotations))
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
    func annotations(document: any PdfDocument) -> [Annotation] { currentAnnotations }
    func annotationEditingAllowed(document: any PdfDocument) -> Bool { annotationsPermitted }

    func applyAnnotationEdit(document: any PdfDocument, edit: AnnotationEdit) throws {
        if let annotationEditError { throw annotationEditError }
        annotationEdits.append(edit)
        var list = currentAnnotations
        switch edit {
        case let .add(kind, page, rect, color):
            list.append(Annotation(id: takeAnnotationId(), page: page, kind: kind, rect: rect, points: [], color: color, contents: nil))
        case let .remove(id):
            list.removeAll { $0.id == id }
        }
        recordAnnotations(list)
    }

    func recordAnnotations(_ list: [Annotation]) {
        history = Array(history.prefix(historyCursor))
            + [Snapshot(info: (try? documentInfo(document: FakeCoreDocument(pages: pages))) ?? initialInfo, pages: currentPages, annotations: list)]
        historyCursor = history.count
    }

    func takeAnnotationId() -> UInt64 {
        defer { nextAnnotationId += 1 }
        return nextAnnotationId
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
