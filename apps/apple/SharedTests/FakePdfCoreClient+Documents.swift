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
