// Page structure: turning, removing, inserting and moving pages. Mirrors the
// Windows facade's `EditPagesAsync` (`PdfDocumentFacade.cs`): the core applies
// the edit to its model, then the render side is re-derived from the pending
// edits (`refreshPreview`) so the viewer can show the new layout, and the
// shell re-reads the page sizes. Each edit is one undo step; an undo or redo
// in a session that ever edited its pages re-derives the layout the same way.
//
// The core work runs on the shell's queue — `refreshPreview` re-serializes the
// whole document — so, like saving, it is a request/result/apply triple.
import Foundation

extension ViewerStore {
    /// What a page-work request does to the model before the relayout.
    enum PageWork {
        case edit(PageEdit)
        /// Every page of each prepared source, appended as one undo step —
        /// `ViewerStore+Import.swift`.
        case importSources([any ImportSource])
        /// A form field's new value — `ViewerStore+Forms.swift`. The value is
        /// painted by the PDF itself, so it needs the same refresh.
        case fillField(id: UInt64, value: FieldValue)
        /// A new form field — `ViewerStore+FormPlacement.swift`.
        case addField(kind: FieldToPlace, page: Int, rect: PdfRect)
        /// A change to a field's structure — `ViewerStore+FormEditing.swift`.
        case editField(FieldEdit)
        /// A change to what a page paints — `ViewerStore+ContentEdit.swift`.
        case editContent(ContentEdit)
        /// Nothing: re-derive the layout after an undo or redo.
        case refresh
    }

    /// Page work in flight.
    struct PageWorkRequest {
        let generation: UInt
        let document: any PdfDocument
        let work: PageWork
    }

    struct PageWorkOutcome {
        /// Whether the work reached the core's history — true even when the
        /// refresh after it failed, because the step is there to undo.
        let applied: Bool
        let pages: Result<[PageDimensions], EditFailure>
        /// What the status line reports for an applied step.
        var done: String?
    }

    var canEditPages: Bool { document != nil && !pageWorkInFlight }

    /// Validates `edit` against the current layout and starts it, or reports
    /// why it cannot run and returns `nil`.
    func beginPageEdit(_ edit: PageEdit) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight else { return nil }
        if let refusal = refusal(for: edit) {
            editStatus = refusal
            return nil
        }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .edit(edit))
    }

    /// The layout refresh an undo or redo owes the viewer, or `nil` when
    /// nothing in this session ever changed the pages.
    func beginPageRefresh() -> PageWorkRequest? {
        guard pagesEdited, let document, !pageWorkInFlight else { return nil }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .refresh)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func pageWorkResult(for request: PageWorkRequest) -> PageWorkOutcome {
        let done: String?
        do {
            switch request.work {
            case let .edit(edit):
                try client.applyPageEdit(document: request.document, edit: edit)
                done = Self.doneMessage(for: edit)
            case let .importSources(sources):
                let added = try client.importPrepared(document: request.document, sources: sources)
                done = "Imported \(added) \(added == 1 ? "page" : "pages") from \(sources.count) \(sources.count == 1 ? "PDF" : "PDFs")."
            case let .fillField(id, value):
                try client.setFieldValue(document: request.document, fieldId: id, value: value)
                done = "Field filled in."
            case let .addField(kind, page, rect):
                try client.addField(document: request.document, kind: kind, page: page, rect: rect)
                done = "\(kind.name) placed."
            case let .editField(edit):
                try client.applyFieldEdit(document: request.document, edit: edit)
                done = edit.done
            case let .editContent(edit):
                try client.applyContentEdit(document: request.document, edit: edit)
                done = edit.done
            case .refresh:
                done = nil
            }
        } catch {
            return PageWorkOutcome(applied: false, pages: .failure(Self.editFailure(error)))
        }
        let applied = done != nil
        do {
            try client.refreshPreview(document: request.document)
        } catch {
            return PageWorkOutcome(applied: applied, pages: .failure(Self.editFailure(error)), done: done)
        }
        return PageWorkOutcome(applied: applied, pages: .success(client.pageDimensions(document: request.document)), done: done)
    }

    func applyPageWork(_ outcome: PageWorkOutcome, for request: PageWorkRequest) {
        guard request.generation == generation else { return }
        pageWorkInFlight = false
        if outcome.applied {
            pagesEdited = true
        }
        switch outcome.pages {
        case let .success(pages):
            bumpGeneration()
            showPages(pages)
            // The page in view may be gone (a delete) or have moved; a held
            // navigation request named a position in the old layout.
            requestedPage = nil
            currentPage = min(currentPage, max(pages.count - 1, 0))
            // Fields travel with their pages, and a fill changes a value.
            refreshFormFields()
            // What a page paints may have changed; read it again on need.
            forgetPageContent()
            noteContentWork(request.work)
            if let done = outcome.done {
                recordEdit(status: "\(done) Changes are pending save.")
            }
        case let .failure(failure):
            if outcome.applied {
                recordEdit(status: failure.localizedDescription)
            } else {
                editStatus = failure.localizedDescription
            }
        }
    }

    private func refusal(for edit: PageEdit) -> String? {
        let count = pageSlots.count
        switch edit {
        case let .rotate(page, degrees):
            // A turn by a whole number of revolutions changes nothing and
            // would still cost an undo step.
            guard (0..<count).contains(page), degrees % 90 == 0, degrees % 360 != 0 else { return "Invalid page position." }
        case let .remove(page):
            guard (0..<count).contains(page) else { return "Invalid page position." }
            guard count > 1 else { return "A document needs at least one page." }
        case let .insertBlank(at, _):
            guard (0...count).contains(at) else { return "Invalid page position." }
        case let .move(from, to):
            guard (0..<count).contains(from), (0..<count).contains(to), from != to else { return "Invalid page position." }
        }
        return nil
    }

    private static func doneMessage(for edit: PageEdit) -> String {
        switch edit {
        case .rotate: return "Page rotated."
        case .remove: return "Page deleted."
        case .insertBlank: return "Blank page added."
        case .move: return "Page moved."
        }
    }
}
