// The document-operation doubles of `FakePdfCoreClient`: extract, split,
// export, compress, signing and import. Their scripted answers are the
// stored properties declared in FakePdfCoreClient.swift.
import Foundation
@testable import Vitela

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
        if document is FakeSnapshotDocument { exportedFromSnapshot += 1 }
        if let exportError, exportedImages.count > failExportsAfter { throw exportError }
        return Data("image-\(page)".utf8)
    }

    func outputSnapshot(document: any PdfDocument) throws -> any PdfDocument {
        snapshotsTaken += 1
        if let snapshotError { throw snapshotError }
        return FakeSnapshotDocument(pages: document.pages)
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

    func openTokenSigningSource(modulePath: String, pin: String?) throws -> any SigningSource {
        tokenOpens.append((modulePath, pin))
        if let tokenError { throw tokenError }
        return FakeSigningSource(identities: signingIdentities)
    }

    /// Lists the store's identities, as the core does; signing never asks it.
    func openNativeSigningSource(_ store: any NativeSigningStore) throws -> any SigningSource {
        FakeSigningSource(identities: try store.identities().map { SigningIdentity(id: $0.id, displayName: $0.displayName) })
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

struct FakeImportSource: ImportSource {
    var pages: [PageDimensions]
    var warnings: [String] = []
    var password: String?

    var pageCount: Int { pages.count }
}

extension FakePdfCoreClient {
    func importRefusal(document: any PdfDocument) -> String? { importRefusalText }

    func prepareImport(bytes: Data, password: String?) throws -> any ImportSource {
        preparedPasswords.append(password)
        guard let source = importSources[bytes] else { throw EditFailure.failed("Not a PDF.") }
        if let expected = source.password, expected != password {
            throw password == nil ? ViewerFailure.passwordRequired : ViewerFailure.wrongPassword
        }
        return source
    }

    func importPrepared(document: any PdfDocument, sources: [any ImportSource]) throws -> Int {
        if let importError { throw importError }
        let added = sources.compactMap { $0 as? FakeImportSource }.flatMap(\.pages)
        appendToHistory(pages: currentPages + added)
        return added.count
    }
}

extension FakePdfCoreClient {
    func readPageContent(document: any PdfDocument, page: Int) throws -> PageContent {
        pageContentReads.append(page)
        return pageContents[page] ?? PageContent(textRuns: [], images: [])
    }

    func imageReplacementRefusal(document: any PdfDocument, image: ContentImage) -> String? { imageReplaceRefusal }

    func textRunHasPendingMove(document: any PdfDocument, run: ContentTextRun) -> Bool {
        movedRuns.contains(run.id)
    }

    /// Records the edit as one history step, and makes the page say what the
    /// edit says, so a re-read after it sees the change.
    func applyContentEdit(document: any PdfDocument, edit: ContentEdit) throws {
        if let contentEditError { throw contentEditError }
        contentEdits.append(edit)
        switch edit {
        case let .retypeText(run, text):
            replaceRun(run) { [ContentTextRun(id: $0.id, page: $0.page, bbox: $0.bbox, resourceFontName: $0.resourceFontName, fontKind: $0.fontKind, text: text)] }
        case let .removeText(run):
            replaceRun(run) { _ in [] }
        case let .insertText(page, rect, text):
            let content = pageContents[page] ?? PageContent(textRuns: [], images: [])
            let inserted = ContentTextRun(id: 1_000 + UInt64(content.textRuns.count), page: page, bbox: rect, resourceFontName: "VitelaText", fontKind: .standard14, text: text)
            pageContents[page] = PageContent(textRuns: content.textRuns + [inserted], images: content.images)
        case let .moveImage(image, rect), let .resizeImage(image, rect):
            replaceImage(image) { [ContentImage(id: $0.id, page: $0.page, bbox: rect, resourceXObjectName: $0.resourceXObjectName)] }
        case let .removeImage(image):
            replaceImage(image) { _ in [] }
        case .replaceImage:
            break
        case let .insertImage(page, rect, _):
            let content = pageContents[page] ?? PageContent(textRuns: [], images: [])
            let inserted = ContentImage(id: 2_000 + UInt64(content.images.count), page: page, bbox: rect, resourceXObjectName: "VitelaImage")
            pageContents[page] = PageContent(textRuns: content.textRuns, images: content.images + [inserted])
        case let .moveText(run, rect):
            movedRuns.insert(run.id)
            replaceRun(run) { [ContentTextRun(id: $0.id, page: $0.page, bbox: rect, resourceFontName: $0.resourceFontName, fontKind: $0.fontKind, text: $0.text)] }
        }
        recordAnnotations(currentAnnotations)
    }

    private func replaceImage(_ image: ContentImage, with change: (ContentImage) -> [ContentImage]) {
        guard let content = pageContents[image.page] else { return }
        pageContents[image.page] = PageContent(
            textRuns: content.textRuns,
            images: content.images.flatMap { $0.id == image.id ? change($0) : [$0] }
        )
    }

    private func replaceRun(_ run: ContentTextRun, with change: (ContentTextRun) -> [ContentTextRun]) {
        guard let content = pageContents[run.page] else { return }
        pageContents[run.page] = PageContent(
            textRuns: content.textRuns.flatMap { $0.id == run.id ? change($0) : [$0] },
            images: content.images
        )
    }
}
