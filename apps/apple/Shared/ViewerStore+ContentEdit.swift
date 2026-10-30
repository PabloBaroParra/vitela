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
/// Everything Edit Text keeps between clicks. The store publishes it as one
/// value; `ViewerStore`'s accessors below read and write its fields.
struct ContentEditState {
    var mode = false
    /// Each page's editable content, read on first need in the mode.
    var pages: [Int: PageContent] = [:]
    var requested = Set<Int>()
    /// A press on a line: a click until it travels, then a move.
    var drag: ContentDrag?
    /// The image clicked last, whose handles show.
    var selectedImage: ContentImage?
    /// A press on the selected image: a move, or a resize by a corner.
    var imageDrag: ImageDrag?
    /// Images moved, resized or deleted since the document was opened, by
    /// page and id: Windows refuses a second edit of one until the document
    /// is saved and reopened.
    var editedImages = Set<ContentImageKey>()
    /// Images the core was asked about on selection, and those it said it
    /// could not read back for a replace — asked once per image, since it is
    /// a property of the bytes, and forgotten with every new revision.
    var replaceChecked = Set<ContentImageKey>()
    var unreplaceableImages = Set<ContentImageKey>()
    /// The vector path clicked last, whose handles show (B25 T-206).
    var selectedPath: ContentPath?
    /// A press on a path: a move, or a reshape by the selected path's corner.
    var pathDrag: PathDrag?
    /// The path an edit just landed on, selected again at its new box once
    /// its page is read back, so the next drag starts from what is painted.
    var reselectPath: ContentImageKey?
}

/// An image's or a path's identity across reads of its page.
struct ContentImageKey: Hashable {
    let page: Int
    let id: UInt64
}

/// A press on a line in Edit Text mode. It stays a click — which opens the
/// line — until the pointer travels `ContentEditing.clickEpsilon` screen
/// points; from then on it drags the line, as in Linux's and Windows' editor.
struct ContentDrag: Equatable {
    /// The run pressed, and the block it reads as (B25 T-207) — the unit
    /// that moves and opens; a block of one when it joins nothing.
    let run: ContentTextRun
    let block: ContentTextBlock
    let origin: PdfPoint
    var current: PdfPoint
    /// Screen points per page point, to measure the travel on screen.
    let zoom: Double
    var moving = false

    /// Where the block would land if released here.
    var previewRect: PdfRect {
        PdfRect(x: block.bbox.x + current.x - origin.x, y: block.bbox.y + current.y - origin.y,
                width: block.bbox.width, height: block.bbox.height)
    }
}

/// What releasing a press on a line comes to.
enum ContentRelease {
    /// A click: open the block to retype or delete it.
    case open(ContentTextBlock)
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
    var contentEditMode: Bool {
        get { contentEdit.mode }
        set { contentEdit.mode = newValue }
    }

    var pageContent: [Int: PageContent] {
        get { contentEdit.pages }
        set { contentEdit.pages = newValue }
    }

    var pageContentRequested: Set<Int> {
        get { contentEdit.requested }
        set { contentEdit.requested = newValue }
    }

    var contentDrag: ContentDrag? {
        get { contentEdit.drag }
        set { contentEdit.drag = newValue }
    }

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
        contentEdit.imageDrag = nil
        contentEdit.selectedImage = nil
        contentEdit.pathDrag = nil
        contentEdit.selectedPath = nil
        contentEdit.reselectPath = nil
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
        reselectPath(on: request.page, in: content)
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
        guard contentEditMode, !pageWorkInFlight, let run = textRun(page: page, at: point, zoom: zoom),
              let content = pageContent[page]
        else { return nil }
        contentDrag = ContentDrag(run: run, block: content.block(containing: run), origin: point, current: point, zoom: max(zoom, 0.01))
        return run
    }

    /// A press on `page` in the mode, for whichever of a line or an image it
    /// meant — Android's order (`contentAt`), with the selected image's
    /// handles first as on Windows: the selected image's corners and body,
    /// then a line the press is inside, then an image it is inside, then the
    /// nearest line within reach. `false` when it meant nothing.
    func beginContentPress(page: Int, at point: PdfPoint, zoom: Double) -> Bool {
        guard contentEditMode, !pageWorkInFlight, let content = pageContent[page] else { return false }
        // A path edit that never landed must not reselect after a later one.
        contentEdit.reselectPath = nil
        if beginImageDrag(page: page, at: point, zoom: zoom, onlySelected: true) { return true }
        if let run = ContentEditing.textRun(at: point, in: content.textRuns, reach: 0) {
            contentEdit.selectedImage = nil
            contentEdit.selectedPath = nil
            contentDrag = ContentDrag(run: run, block: content.block(containing: run), origin: point, current: point, zoom: max(zoom, 0.01))
            return true
        }
        if beginImageDrag(page: page, at: point, zoom: zoom, onlySelected: false) { return true }
        contentEdit.selectedImage = nil
        // Text outranks a path — a frame encloses its cells — as on Windows.
        if beginPathDrag(page: page, at: point, zoom: zoom) { return true }
        return beginContentDrag(page: page, at: point, zoom: zoom) != nil
    }

    func continueContentDrag(to point: PdfPoint) {
        if contentEdit.imageDrag != nil { return continueImageDrag(to: point) }
        if contentEdit.pathDrag != nil { return continuePathDrag(to: point) }
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
        if contentEdit.imageDrag != nil { return endImageDrag(at: point) }
        if contentEdit.pathDrag != nil { return endPathDrag(at: point) }
        continueContentDrag(to: point)
        guard let drag = contentDrag, let document else { return .nothing }
        contentDrag = nil
        guard drag.moving else {
            if drag.block.runs.contains(where: { client.textRunHasPendingMove(document: document, run: $0) }) {
                editStatus = ContentEditing.movedNotSaved
                return .nothing
            }
            return .open(drag.block)
        }
        let to = drag.previewRect
        guard to != drag.block.bbox else {
            editStatus = ContentEditing.positionUnchanged
            return .nothing
        }
        return beginContentEdit(Self.textMove(drag.block, to: to)).map(ContentRelease.move) ?? .nothing
    }

    func cancelContentDrag() {
        contentDrag = nil
        contentEdit.imageDrag = nil
        contentEdit.pathDrag = nil
    }

    /// Starts `edit` through the page-work pipeline, or says why not.
    func beginContentEdit(_ edit: ContentEdit) -> PageWorkRequest? {
        guard let document, !pageWorkInFlight else { return nil }
        let retype: (old: String, new: String)?
        switch edit {
        case let .retypeText(run, text): retype = (run.text, text)
        case let .retypeTextBlock(block, text): retype = (block.text, text)
        default: retype = nil
        }
        if let retype {
            if retype.new.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                editStatus = ContentEditing.emptyRetype
                return nil
            }
            if retype.new == retype.old { return nil }
        }
        pageWorkInFlight = true
        return PageWorkRequest(generation: generation, document: document, work: .editContent(edit))
    }

    /// The edits a block's editor and drag record: the single-run edits for a
    /// block of one, as before T-207, the block commands otherwise — Windows'
    /// `IsMultiRun` split.
    static func textMove(_ block: ContentTextBlock, to: PdfRect) -> ContentEdit {
        block.isMultiRun
            ? .moveTextBlock(block, dx: to.x - block.bbox.x, dy: to.y - block.bbox.y)
            : .moveText(block.first, to: to)
    }

    static func textRetype(_ block: ContentTextBlock, to text: String) -> ContentEdit {
        block.isMultiRun ? .retypeTextBlock(block, to: text) : .retypeText(block.first, to: text)
    }

    static func textRemoval(_ block: ContentTextBlock) -> ContentEdit {
        block.isMultiRun ? .removeTextBlock(block) : .removeText(block.first)
    }

    /// Inserts `text` at `size` (as typed) with its top-left corner at
    /// `point` on `page`. Returns why the sheet must stay open, or `nil`
    /// once the insertion is on its way.
    func beginTextInsert(page: Int, at point: PdfPoint, text: String, size: String) -> (problem: String?, request: PageWorkRequest?) {
        guard ContentEditing.insertableText(text) else { return (ContentEditing.insertEmpty, nil) }
        guard let points = Double(size.trimmingCharacters(in: .whitespaces)),
              let rect = ContentEditing.insertedTextRect(at: point, size: points)
        else { return (ContentEditing.insertSizeInvalid, nil) }
        return (nil, beginContentEdit(.insertText(page: page, rect: rect, text: text)))
    }

    /// After anything that may change what a page paints.
    func forgetPageContent() {
        contentDrag = nil
        contentEdit.imageDrag = nil
        contentEdit.selectedImage = nil
        contentEdit.pathDrag = nil
        contentEdit.selectedPath = nil
        pageContent = [:]
        pageContentRequested = []
    }

    /// Called by `resetEditing` once a document has replaced the previous one.
    func resetContentEditing() {
        contentEdit = ContentEditState()
    }
}
