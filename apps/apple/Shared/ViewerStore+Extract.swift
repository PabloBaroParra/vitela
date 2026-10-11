// Extract pages: a typed range of the open document's pages, out into a new
// PDF, leaving the open document exactly as it was. Mirrors the Windows
// facade's `PlanExtractPagesAsync`/`ExtractPagesAsync`
// (`PdfDocumentFacade.ExtractPages.cs`) and its `ExtractPagesWording`: the
// plan checks both permission gates before the range grammar ever runs, so a
// refusal is shown while the reader can still correct it — not after they
// picked a destination.
import Foundation

extension ViewerStore {
    struct ExtractPlan: Equatable {
        /// Zero-based, ascending, deduplicated.
        let pages: [Int]
        let sourceIsSigned: Bool
    }

    /// Bytes for a plan, computed off the main thread.
    struct ExtractRequest {
        let generation: UInt
        let document: any PdfDocument
        let plan: ExtractPlan
    }

    /// Checks the gates, then the typed range. Cheap: no page is copied.
    func planExtract(range: String) -> Result<ExtractPlan, EditFailure> {
        guard let document else { return .failure(.failed("The document is no longer open.")) }
        // Deliberately not the page-assembly permission: an extraction
        // changes nothing about the open document — the same rationale the
        // core's `extract` module and Windows' plan give.
        guard client.textExtractionAllowed(document: document) else {
            return .failure(.notPermitted("This document does not permit extracting its pages."))
        }
        guard client.fullRewriteAllowed(document: document) else {
            return .failure(.notPermitted(
                "Extracting pages rewrites the whole file, which this document's encryption or available credentials do not allow."
            ))
        }
        guard !pageSlots.isEmpty else { return .failure(.failed("This document has no pages to extract.")) }
        guard !range.trimmingCharacters(in: .whitespaces).isEmpty else {
            return .failure(.failed("Type which pages to extract, for example 1-3,7."))
        }
        do {
            let pages = try client.parsePageSelection(range, totalPages: pageSlots.count)
            return .success(ExtractPlan(pages: pages, sourceIsSigned: client.extractSourceIsSigned(document: document)))
        } catch {
            return .failure(Self.editFailure(error))
        }
    }

    func beginExtract(_ plan: ExtractPlan) -> ExtractRequest? {
        guard let document else { return nil }
        return ExtractRequest(generation: generation, document: document, plan: plan)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func extractResult(for request: ExtractRequest) -> Result<Data, EditFailure> {
        Result { try client.extractPages(document: request.document, pages: request.plan.pages) }
            .mapError(Self.editFailure)
    }

    /// Windows' `ExtractPagesWording.Summary`, word for word.
    static func extractSummary(count: Int, destination: String, sourceIsSigned: Bool) -> String {
        var summary = "Extracted \(count) \(count == 1 ? "page" : "pages") to \(destination)."
        if sourceIsSigned {
            summary += " The original document is signed, so the extracted PDF's signature no longer verifies."
        }
        return summary
    }
}
