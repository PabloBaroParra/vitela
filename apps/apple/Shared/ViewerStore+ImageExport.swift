// Export pages as images: chosen pages, written as PNG or JPEG files into a
// folder, leaving the open document as it was. Mirrors the Windows facade's
// `PlanImageExportAsync` (`PdfDocumentFacade.ImageExport.cs`), its limits
// (`ImageExportLimits`) and wording (`ImageExportWording`): every choice is
// checked while the dialog is still open, so a bad range or an oversized
// page is fixed where it was typed rather than after a folder was picked.
import Foundation

extension ViewerStore {
    /// Windows' `ImageExportLimits`.
    static let exportDpiRange = 72...400
    static let defaultExportDpi = 150

    enum ImageExportPages: Equatable {
        case all
        /// The page at the top of the viewport.
        case current
        /// A typed one-based range, `"1-3,7"`.
        case range(String)
    }

    struct ImageExportFile: Equatable {
        let page: Int
        let fileName: String
    }

    struct ImageExportPlan: Equatable {
        let files: [ImageExportFile]
        let dpi: Int
        let format: ImageFormat
    }

    struct ImageExportRequest {
        let generation: UInt
        let document: any PdfDocument
        let file: ImageExportFile
        let dpi: Int
        let format: ImageFormat
    }

    func planImageExport(
        pages: ImageExportPages, dpi: Int, format: ImageFormat, documentName: String
    ) -> Result<ImageExportPlan, EditFailure> {
        guard Self.exportDpiRange.contains(dpi) else {
            return .failure(.failed(
                "Choose a resolution between \(Self.exportDpiRange.lowerBound) and \(Self.exportDpiRange.upperBound) DPI."
            ))
        }
        guard let document else { return .failure(.failed("The document is no longer open.")) }
        guard client.textExtractionAllowed(document: document) else {
            return .failure(.notPermitted("This document does not permit extracting its pages as images."))
        }
        let total = pageSlots.count
        guard total > 0 else { return .failure(.failed("This document has no pages to export.")) }
        let chosen: [Int]
        switch pages {
        case .all:
            chosen = Array(0..<total)
        case .current:
            chosen = [min(currentPage, total - 1)]
        case let .range(text):
            do {
                chosen = try client.parsePageSelection(text, totalPages: total)
            } catch {
                return .failure(Self.editFailure(error))
            }
        }
        if let oversized = client.firstPageTooLargeToExport(document: document, pages: chosen, dpi: dpi) {
            return .failure(.failed("Page \(oversized + 1) is too large to export at \(dpi) DPI. Choose a lower resolution."))
        }
        let files = chosen.map {
            ImageExportFile(
                page: $0,
                fileName: client.pageImageFileName(documentName: documentName, page: $0, totalPages: total, format: format)
            )
        }
        return .success(ImageExportPlan(files: files, dpi: dpi, format: format))
    }

    func beginImageExport(_ file: ImageExportFile, of plan: ImageExportPlan) -> ImageExportRequest? {
        guard let document else { return nil }
        return ImageExportRequest(generation: generation, document: document, file: file, dpi: plan.dpi, format: plan.format)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func imageExportResult(for request: ImageExportRequest) -> Result<Data, EditFailure> {
        Result {
            try client.exportPageImage(
                document: request.document, page: request.file.page, dpi: request.dpi, format: request.format
            )
        }
        .mapError(Self.editFailure)
    }

    /// Windows' `ImageExportWording.Summary`.
    static func imageExportSummary(count: Int, format: ImageFormat, folder: String) -> String {
        "Exported \(count) \(count == 1 ? "page" : "pages") as \(format.name) to \(folder)."
    }
}
