// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Edit Text mode: click a line the page itself paints to retype or delete
/// it — Windows' `MainWindow.ContentEdit.cs`, Linux's `content_edit/`, and
/// Android's `ContentEditing.kt`, whose dialog-based retype this follows.
///
/// Nothing is drawn in place of the words: only the renderer can paint them,
/// so an edit goes through the page-work pipeline (`ViewerStore+Pages.swift`)
/// — applied, the preview re-derived, the pages re-rendered — and is one
/// undoable step. What the shell draws is each run's outline, from the
/// content as the core last reported it.
struct PageContentRequest {
    let generation: UInt
    let document: any PdfDocument
    let page: Int
}

extension ViewerStore {
    /// Arms or disarms the mode. One mode owns a page click at a time, so an
    /// armed annotation tool, Edit fields and a text selection are dropped.
    func setContentEditMode(_ on: Bool) {
        guard document != nil else { return }
        guard !on || editingAllowed else {
            editStatus = ContentEditing.forbidden
            return
        }
        contentEditMode = on
        guard on else {
            editStatus = ContentEditing.off
            return
        }
        armedTool = nil
        annotationDraft = nil
        armedStampImage = nil
        selectedAnnotationId = nil
        formEditMode = false
        armedField = nil
        selectedFieldId = nil
        selection = nil
        editStatus = ContentEditing.armed
    }

    /// Reads `page`'s content in the mode, once; `nil` when it is known or
    /// on its way.
    func beginPageContent(page: Int) -> PageContentRequest? {
        guard contentEditMode, let document, pageSlots.indices.contains(page),
              pageContent[page] == nil, !pageContentRequested.contains(page)
        else { return nil }
        pageContentRequested.insert(page)
        return PageContentRequest(generation: generation, document: document, page: page)
    }

    /// Thread-safe: only the immutable client and the request's document.
    func pageContentResult(for request: PageContentRequest) -> Result<PageContent, EditFailure> {
        Result { try client.readPageContent(document: request.document, page: request.page) }.mapError(Self.editFailure)
    }

    /// A page that cannot be read stays requested, so it is not retried on
    /// every scroll; it simply offers nothing to click.
    func applyPageContent(_ result: Result<PageContent, EditFailure>, for request: PageContentRequest) {
        guard request.generation == generation, case let .success(content) = result else { return }
        pageContent[request.page] = content
    }

    /// The run a click at `point` on `page` meant, at `zoom` — reach is in
    /// screen points, so it stays a finger's width at any zoom.
    func textRun(page: Int, at point: PdfPoint, zoom: Double) -> ContentTextRun? {
        guard let runs = pageContent[page]?.textRuns else { return nil }
        return ContentEditing.textRun(at: point, in: runs, reach: ContentEditing.reach / max(zoom, 0.01))
    }

    /// Starts `edit` through the page-work pipeline, or says why not.
    func beginContentEdit(_ edit: ContentEdit) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight else { return nil }
        if case let .retypeText(run, text) = edit {
            if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                editStatus = ContentEditing.emptyRetype
                return nil
            }
            if text == run.text { return nil }
        }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .editContent(edit))
    }

    /// After anything that may change what a page paints.
    func forgetPageContent() {
        pageContent = [:]
        pageContentRequested = []
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetContentEditing() {
        contentEditMode = false
        forgetPageContent()
    }
}
