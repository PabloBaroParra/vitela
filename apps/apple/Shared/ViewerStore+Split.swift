// Split: typed cut points turn the open document into several PDFs, each
// part one extraction (`ViewerStore+Extract.swift`). Mirrors the Windows
// facade's `PlanSplitPagesAsync` (`PdfDocumentFacade.SplitPages.cs`) and the
// wording of `MainWindow.SplitPages.cs`; the cut grammar and the part file
// names are the core's `plan_split`, shared with the Linux shell.
import Foundation

extension ViewerStore {
    struct SplitPlan: Equatable {
        let parts: [SplitPart]
        let sourceIsSigned: Bool
    }

    /// Checks the gates, then plans the parts for `cuts`. `documentName`
    /// names the files ("report.pdf" → the core's "report-…" names).
    func planSplit(cuts: String, documentName: String) -> Result<SplitPlan, EditFailure> {
        guard let document else { return .failure(.failed("The document is no longer open.")) }
        guard client.textExtractionAllowed(document: document) else {
            return .failure(.notPermitted("This document does not permit extracting its pages."))
        }
        guard client.fullRewriteAllowed(document: document) else {
            return .failure(.notPermitted(
                "Splitting pages rewrites the whole file, which this document's encryption or available credentials do not allow."
            ))
        }
        guard pageSlots.count >= 2 else { return .failure(.failed("A document of one page cannot be split.")) }
        do {
            let parts = try client.planSplit(cuts, totalPages: pageSlots.count, documentName: documentName)
            return .success(SplitPlan(parts: parts, sourceIsSigned: client.extractSourceIsSigned(document: document)))
        } catch {
            return .failure(Self.editFailure(error))
        }
    }

    /// One part's extraction, ready for `extractResult(for:)`.
    func beginSplitPart(_ part: SplitPart, of plan: SplitPlan) -> ExtractRequest? {
        beginExtract(ExtractPlan(pages: Array(part.first...part.last), sourceIsSigned: plan.sourceIsSigned))
    }

    static func splitSummary(written: Int, folder: String, sourceIsSigned: Bool) -> String {
        var summary = "Split into \(written) PDFs in \(folder)."
        if sourceIsSigned {
            summary += " The original document is signed, so the signature in each part no longer verifies."
        }
        return summary
    }

    static func splitStopped(written: Int, part: String, reason: String) -> String {
        "Split stopped after \(written) written: \(part) could not be written: \(reason)"
    }
}
