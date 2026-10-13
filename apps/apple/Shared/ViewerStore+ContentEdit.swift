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
/// A press on a line in Edit Text mode. It stays a click — which opens the
/// line — until the pointer travels `ContentEditing.clickEpsilon` screen
/// points; from then on it drags the line, as in Linux's and Windows' editor.
struct ContentDrag: Equatable {
    let run: ContentTextRun
    let origin: PdfPoint
    var current: PdfPoint
    /// Screen points per page point, to measure the travel on screen.
    let zoom: Double
    var moving = false

    /// Where the line would land if released here.
    var previewRect: PdfRect {
        PdfRect(x: run.bbox.x + current.x - origin.x, y: run.bbox.y + current.y - origin.y,
                width: run.bbox.width, height: run.bbox.height)
    }
}

/// What releasing a press on a line comes to.
enum ContentRelease {
    /// A click: open the line to retype or delete it.
    case open(ContentTextRun)
    /// A drag that moved the line: the move to run as page work.
    case move(ViewerStore.PageWorkRequest)
    /// Nothing to run; the status line says why when there is a why.
    case nothing
}

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
        contentDrag = nil
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

    /// A press on `page` in the mode: grabs the line under it, or returns
    /// `nil` when there is none.
    func beginContentDrag(page: Int, at point: PdfPoint, zoom: Double) -> ContentTextRun? {
        guard contentEditMode, !pageWorkInFlight, let run = textRun(page: page, at: point, zoom: zoom) else { return nil }
        contentDrag = ContentDrag(run: run, origin: point, current: point, zoom: max(zoom, 0.01))
        return run
    }

    func continueContentDrag(to point: PdfPoint) {
        guard var drag = contentDrag else { return }
        drag.current = point
        let travel = ((point.x - drag.origin.x) * (point.x - drag.origin.x) + (point.y - drag.origin.y) * (point.y - drag.origin.y)).squareRoot()
        if travel * drag.zoom >= ContentEditing.clickEpsilon { drag.moving = true }
        contentDrag = drag
    }

    /// Ends the press at `point`. A line moved and not saved since cannot be
    /// retyped — the core keeps its old text where its move points — so a
    /// click on one says so instead of opening it, as Windows does.
    func endContentDrag(at point: PdfPoint) -> ContentRelease {
        continueContentDrag(to: point)
        guard let drag = contentDrag, let document else { return .nothing }
        contentDrag = nil
        guard drag.moving else {
            if client.textRunHasPendingMove(document: document, run: drag.run) {
                editStatus = ContentEditing.movedNotSaved
                return .nothing
            }
            return .open(drag.run)
        }
        let to = drag.previewRect
        guard to != drag.run.bbox else {
            editStatus = ContentEditing.positionUnchanged
            return .nothing
        }
        return beginContentEdit(.moveText(drag.run, to: to)).map(ContentRelease.move) ?? .nothing
    }

    func cancelContentDrag() {
        contentDrag = nil
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
        contentDrag = nil
        pageContent = [:]
        pageContentRequested = []
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetContentEditing() {
        contentEditMode = false
        forgetPageContent()
    }
}
