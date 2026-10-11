// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Extracting pages — and splitting, which is several extractions — through
/// the generated UniFFI API. The range and cut grammars, both permission
/// gates and the prune are the core's (`pdf-ffi`'s `extract`
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

    func planSplit(_ cuts: String, totalPages: Int, documentName: String) throws -> [SplitPart] {
        do {
            return try Vitela.planSplit(typed: cuts, totalPages: UInt32(totalPages), documentName: documentName).map {
                SplitPart(first: Int($0.first), last: Int($0.last), fileName: $0.fileName)
            }
        } catch let FfiError.InvalidPageSelection(detail) {
            throw EditFailure.failed(detail)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}
