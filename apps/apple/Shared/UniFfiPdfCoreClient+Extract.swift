// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Extracting pages through the generated UniFFI API. The range grammar,
/// both permission gates and the prune are the core's (`pdf-ffi`'s `extract`
/// module), shared with the Linux and Windows chains.
extension UniFfiPdfCoreClient {
    func textExtractionAllowed(document: any PdfDocument) -> Bool {
        (try? handle(of: document).textExtractionAllowed()) ?? false
    }

    func fullRewriteAllowed(document: any PdfDocument) -> Bool {
        (try? handle(of: document).fullRewriteAllowed()) ?? false
    }

    func parsePageSelection(_ text: String, totalPages: Int) throws -> [Int] {
        do {
            return try Vitela.parsePageSelection(input: text, totalPages: UInt32(totalPages)).map(Int.init)
        } catch let FfiError.InvalidPageSelection(detail) {
            // The core writes this sentence for the person who typed the
            // range; it is shown as it is.
            throw EditFailure.failed(detail)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }

    func extractPages(document: any PdfDocument, pages: [Int]) throws -> Data {
        let handle = try handle(of: document)
        return try Self.mapEditErrors { try extractPagesToPdf(handle: handle, pages: pages.map(UInt32.init)) }
    }

    func extractSourceIsSigned(document: any PdfDocument) -> Bool {
        guard let handle = try? handle(of: document) else { return false }
        return Vitela.extractSourceIsSigned(handle: handle)
    }
}
