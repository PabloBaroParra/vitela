import Combine
import Foundation

enum ViewerState: Equatable {
    case empty
    case loading
    case loaded
    case error(ViewerFailure)
}

enum PageStatus: Equatable {
    case idle
    case rendering
    case rendered
    case failed(String)
}

struct PageSlot: Equatable {
    let index: Int
    let dimensions: PageDimensions
    var status: PageStatus = .idle
    /// Zoom the current `status` refers to. A slot whose `renderZoom` differs
    /// from the zoom its page is shown at (`zoom(forPage:)`) is stale and will be re-rendered when it is next
    /// on screen — the previous bitmap keeps showing meanwhile, so no flicker.
    var renderZoom: Double?
    var image: RenderedPage?
}

/// A self-contained unit of render work. It carries the document and DPI it was
/// issued for, so the background queue never has to read back into the store —
/// which is what makes `renderResult(for:)` safe to call off the main thread.
struct RenderRequest {
    let generation: UInt
    let page: Int
    let document: any PdfDocument
    let dpi: Int
    let zoom: Double
}

/// One completed document-wide search. Feature logic lives in
/// `ViewerStore+Search.swift`; this is just the state it publishes.
struct SearchSession: Equatable {
    let query: String
    let matches: [SearchMatch]
    var currentIndex: Int
}

/// A drag-select range on one page, in caret indices (`PageCharacters`
/// space). `anchor`/`focus` need not be ordered — see
/// `ViewerStore+Selection.swift`.
struct TextSelection: Equatable {
    let page: Int
    var anchor: Int
    var focus: Int
}

/// Main-thread only, except for `renderResult(for:)`, which is explicitly
/// documented as thread-safe. Every stored property is mutated on main.
final class ViewerStore: ObservableObject {
    static let minimumZoom = 0.5
    static let maximumZoom = 4.0
    static let baseDpi = 72.0

    @Published private(set) var state: ViewerState = .empty
    @Published private(set) var pageSlots: [PageSlot] = []
    @Published private(set) var zoom = 1.0
    // Owned by `ViewerStore+Zoom.swift` (not `private(set)` for the same
    // same-file reason as `search` below).
    @Published var zoomMode = ZoomMode.custom
    @Published var viewport: ViewportSize?
    // Not `private(set)`: `private` extends only to same-file extensions, and
    // `ViewerStore+Search.swift`/`ViewerStore+Selection.swift` mutate these
    // from their own files — same reasoning as `pageCharactersCache` below.
    @Published var search: SearchSession?
    @Published var searchStatus = ""
    @Published var selection: TextSelection?
    // Editing state, owned by `ViewerStore+Editing.swift` (not `private(set)`
    // for the same same-file reason as `search` above).
    /// Bumped by every edit, undo and redo; compared against `savedRevision`
    /// to tell whether the open document has unsaved changes.
    @Published var editRevision = 0
    @Published var savedRevision = 0
    @Published var canUndo = false
    @Published var canRedo = false
    /// `nil` until a document is open.
    @Published var documentInfo: DocumentInfo?
    @Published var editingAllowed = false
    /// The last edit or save outcome, in words for a status line.
    @Published var editStatus = ""
    // Navigation state, owned by `ViewerStore+Navigation.swift` (not
    // `private(set)` for the same same-file reason as `search` above).
    /// The first page visible at the top of the viewport.
    @Published var currentPage = 0
    /// The latest scroll request for the shell's page list to perform.
    @Published var pageNavigation: PageNavigationRequest?
    var navigationSerial: UInt = 0
    /// The page the reader explicitly asked for, held until they scroll away.
    var requestedPage: RequestedPage?
    // Signing state, owned by `ViewerStore+Signing.swift`.
    /// Why the open document cannot be signed, or `nil` when it can.
    @Published var signingRefusal: String?
    /// Whether the open document carries a digital signature.
    @Published var isSigned = false
    // Page-structure state, owned by `ViewerStore+Pages.swift`.
    /// A page edit or preview refresh is running on the shell's queue.
    @Published var pageWorkInFlight = false
    /// Latched by the first page edit: from then on any undo or redo may
    /// change the page layout, so each one re-derives it.
    var pagesEdited = false
    /// Organize-grid pictures by page position — `ViewerStore+Thumbnails.swift`.
    @Published var thumbnails: [Int: RenderedPage] = [:]
    var thumbnailsRequested: Set<Int> = []
    // Annotation state, owned by `ViewerStore+Annotations.swift`.
    @Published var annotations: [Annotation] = []
    @Published var annotationEditingAllowed = false
    @Published var armedTool: AnnotationTool?
    @Published var annotationDraft: AnnotationDraft?
    @Published var pendingNote: PendingNote?
    @Published var pendingFreeText: PendingFreeText?
    @Published var selectedAnnotationId: UInt64?
    @Published var annotationGesture: AnnotationGesture?

    /// Not `private`: `ViewerStore+Search.swift` and `ViewerStore+Selection.swift`
    /// read it to issue their own background-safe requests, the same way this
    /// file's own `beginRender`/`renderResult` split works.
    let client: PdfCoreClient
    private(set) var document: (any PdfDocument)?
    /// Bumped on every `open`. Exposed (read-only) so a view can key each
    /// page row's identity to it — `pageSlots` reuses the same `index`
    /// values across documents, and `ForEach(id: \.index)` alone would treat
    /// a freshly opened document's rows as the *same* views as the previous
    /// document's, so `onAppear` would never refire to request their render.
    private(set) var generation: UInt = 0
    /// The bytes behind the most recent `open` attempt, kept around so a
    /// password prompt can retry without asking the user to re-pick the file.
    private var pendingBytes: Data?
    /// How big the open document's file is — what Compress compares against.
    private(set) var sourceByteCount: Int?
    /// One `PageCharacters` per page, populated lazily by
    /// `ViewerStore+Selection.swift` after that page's bitmap renders. Not
    /// `private` for the same reason as `client`/`document` above.
    var pageCharactersCache: [Int: any PageCharacters] = [:]

    init(client: PdfCoreClient) {
        self.client = client
    }

    func selectionCancelled() {}

    func open(bytes: Data, password: String? = nil) {
        open(bytes: bytes) { try $0.open(bytes: bytes, password: password) }
    }

    /// Opens a file this shell just protected, with both of the passwords it
    /// set — the owner password is what keeps it editable here, as Windows'
    /// `ReopenProtectedAsync` does.
    func open(bytes: Data, userPassword: String, ownerPassword: String) {
        open(bytes: bytes) { try $0.open(bytes: bytes, userPassword: userPassword, ownerPassword: ownerPassword) }
    }

    /// A new, unsaved document with one blank A4 page — Linux's and
    /// Windows' Ctrl+N. Created with its first page already present, as on
    /// Windows: inserting one into an empty document would be a pending edit
    /// before the reader did anything.
    func openBlank() {
        open(bytes: nil) { try $0.createBlankDocument() }
    }

    /// Shows signed output in place of `signed` — the document it was signed
    /// from — opened with that document's credentials.
    func openSigned(_ bytes: Data, from signed: any PdfDocument) {
        open(bytes: bytes) { try $0.reopenSigned(document: signed, bytes: bytes) }
    }

    /// `bytes` is `nil` for a document that never was a file.
    private func open(bytes: Data?, using opener: (PdfCoreClient) throws -> any PdfDocument) {
        generation += 1
        let attemptedGeneration = generation
        pendingBytes = bytes
        state = .loading
        do {
            let opened = try opener(client)
            guard attemptedGeneration == generation else { return }
            document = opened
            // `nil` for a new document: Compress then says there is no file size.
            sourceByteCount = bytes?.count
            showPages(opened.pages)
            resetEditing()
            resetNavigation()
            state = .loaded
        } catch let failure as ViewerFailure {
            guard attemptedGeneration == generation else { return }
            state = .error(failure)
        } catch {
            guard attemptedGeneration == generation else { return }
            state = .error(.openFailed(String(describing: error)))
        }
    }

    /// Shows `pages` as the open document's layout: fresh slots, and nothing
    /// kept that was addressed by page position. A previous layout's page
    /// characters, matches and selection address pages/carets that may no
    /// longer exist or mean the same thing. Called by `open`, and by
    /// `ViewerStore+Pages.swift` after an edit re-laid the pages out, with
    /// `generation` already bumped so renders of the old layout are dropped.
    func showPages(_ pages: [PageDimensions]) {
        pageSlots = pages.enumerated().map { PageSlot(index: $0.offset, dimensions: $0.element) }
        thumbnails = [:]
        thumbnailsRequested = []
        pageCharactersCache = [:]
        search = nil
        searchStatus = ""
        selection = nil
    }

    /// Invalidates everything issued against the current page layout —
    /// renders, searches, character fetches — and gives each page row a new
    /// identity, so views re-request them.
    func bumpGeneration() {
        generation += 1
    }

    /// Retries the last `open` attempt with a password, e.g. after the user
    /// answers a `.passwordRequired`/`.wrongPassword` prompt. A no-op if no
    /// document has been opened yet in this store's lifetime.
    func retryPassword(_ password: String) {
        guard let bytes = pendingBytes else { return }
        open(bytes: bytes, password: password)
    }

    /// Reports a failure that happened before any bytes reached the core (a
    /// failed disk read). The currently open document is deliberately left
    /// alone: a failed attempt to open a second file must not close the first.
    func reportOpenFailure(_ failure: ViewerFailure) {
        state = .error(failure)
    }

    /// Sets an explicit factor for every page, leaving any fit mode.
    func setZoom(_ value: Double) {
        zoomMode = .custom
        zoom = Self.clampZoom(value)
    }

    /// Issues render work for `page`, or returns `nil` when there is nothing to
    /// do — the page is already rendered (or in flight, or failed) at the
    /// current zoom. `force` bypasses that check for an explicit retry.
    func beginRender(page: Int, force: Bool = false) -> RenderRequest? {
        guard pageSlots.indices.contains(page), let document else { return nil }
        let pageZoom = zoom(forPage: page)
        guard force || !isUpToDate(pageSlots[page], at: pageZoom) else { return nil }

        pageSlots[page].status = .rendering
        pageSlots[page].renderZoom = pageZoom
        return RenderRequest(
            generation: generation,
            page: page,
            document: document,
            dpi: dpi(for: pageZoom),
            zoom: pageZoom
        )
    }

    /// Thread-safe: touches only the immutable `client` and the request's own
    /// captured document. Callers run this off the main thread on purpose.
    func renderResult(for request: RenderRequest) -> Result<RenderedPage, ViewerFailure> {
        do {
            let page = try client.render(document: request.document, page: request.page, dpi: request.dpi)
            return .success(page)
        } catch let failure as ViewerFailure {
            return .failure(failure)
        } catch {
            return .failure(.renderFailed(page: request.page, message: String(describing: error)))
        }
    }

    func applyRender(_ result: Result<RenderedPage, ViewerFailure>, for request: RenderRequest) {
        guard request.generation == generation, pageSlots.indices.contains(request.page) else { return }
        // The result describes the zoom it was rendered at, not the zoom that is
        // current now; recording it is what lets a stale slot re-render later.
        pageSlots[request.page].renderZoom = request.zoom

        switch result {
        case let .success(image):
            guard isDisplayable(image) else {
                // Slot-local, not document-wide: one unusable page must not tear
                // down a document whose other pages render fine.
                pageSlots[request.page].status = .failed(ViewerFailure.invalidImage(page: request.page).message)
                return
            }
            pageSlots[request.page].image = image
            pageSlots[request.page].status = .rendered
        case let .failure(failure):
            pageSlots[request.page].status = .failed(failure.message)
        }
    }

    func retry(page: Int) -> RenderRequest? {
        beginRender(page: page, force: true)
    }

    func dpi(for zoom: Double) -> Int {
        Int((Self.baseDpi * zoom).rounded())
    }

    private func isUpToDate(_ slot: PageSlot, at pageZoom: Double) -> Bool {
        guard slot.renderZoom == pageZoom else { return false }
        switch slot.status {
        case .rendering, .rendered, .failed: return true
        case .idle: return false
        }
    }

    /// Core Graphics reads `stride * height` bytes out of the provider without
    /// bounds-checking, so the buffer length has to be verified here — a short
    /// buffer is an out-of-bounds read, not a blank page. Not `private`:
    /// `ViewerStore+Print.swift` checks its printer bitmaps, and thumbnails
    /// are checked, the same way.
    func isDisplayable(_ image: RenderedPage) -> Bool {
        image.width > 0
            && image.height > 0
            && image.stride >= image.width * 4
            && image.rgba.count >= image.stride * image.height
    }
}
