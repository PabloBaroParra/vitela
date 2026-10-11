// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Exporting pages as images through the generated UniFFI API.
extension UniFfiPdfCoreClient {
    func exportPageImage(document: any PdfDocument, page: Int, dpi: Int, format: ImageFormat) throws -> Data {
        let handle = try handle(of: document)
        return try Self.mapEditErrors {
            try Vitela.exportPageImage(handle: handle, pageIndex: UInt32(page), dpi: UInt32(dpi), format: FfiExportFormat(format))
        }
    }

    func firstPageTooLargeToExport(document: any PdfDocument, pages: [Int], dpi: Int) -> Int? {
        guard let handle = try? handle(of: document) else { return nil }
        return Vitela.firstPageTooLargeToExport(handle: handle, pages: pages.map(UInt32.init), dpi: UInt32(dpi)).map(Int.init)
    }

    func pageImageFileName(documentName: String, page: Int, totalPages: Int, format: ImageFormat) -> String {
        Vitela.pageImageFileName(
            documentName: documentName, pageIndex: UInt32(page), totalPages: UInt32(totalPages), format: FfiExportFormat(format)
        )
    }
}

private extension FfiExportFormat {
    init(_ format: ImageFormat) {
        self = format == .png ? .png : .jpeg
    }
}
