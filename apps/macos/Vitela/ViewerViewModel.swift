import AppKit
import Combine
import Foundation
import UniformTypeIdentifiers

final class ViewerViewModel: ObservableObject {
    @Published private(set) var title = ViewerViewModel.windowTitle(for: .empty)
    /// The open document's file name, used to suggest a name when saving.
    /// Not `private(set)`: `ViewerViewModel+FileDrop.swift` names what it
    /// opens, and `ViewerViewModel+Protect.swift` renames it to the protected
    /// file it reopens.
    @Published var documentName: String?
    @Published var isSaving = false
    /// Whether the Go to page sheet is up — see `ViewerViewModel+Navigation.swift`.
    @Published var isGoToPagePresented = false
    /// A shell-level message for the status line — a drop that could not be
    /// opened, say — as opposed to the store's edit and save outcomes.
    @Published var notice = ""
    /// Whether the Extract pages sheet is up — `ViewerViewModel+Extract.swift`.
    @Published var isExtractPresented = false
    /// Whether the Split PDF sheet is up — `ViewerViewModel+Split.swift`.
    @Published var isSplitPresented = false
    /// Whether the Export images sheet is up — `ViewerViewModel+ImageExport.swift`.
    @Published var isExportImagesPresented = false
    /// Whether the Compress sheet is up — `ViewerViewModel+Compress.swift`.
    @Published var isCompressPresented = false
    /// Whether the Protect sheet is up — `ViewerViewModel+Protect.swift`.
    @Published var isProtectPresented = false
    /// Whether the Add watermark sheet is up — `ViewerViewModel+Watermark.swift`.
    @Published var isWatermarkPresented = false
    /// Whether the Organize grid stands in for the reader — see
    /// `ViewerViewModel+Pages.swift`.
    @Published var isOrganizing = false
    /// The page text being retyped, while its sheet is up —
    /// `ViewerViewModel+ContentEdit.swift`.
    @Published var contentTextEditor: ContentTextBlock?
    /// What the last ⌘C or ⌘X in Edit Content held, across documents, and
    /// where the pointer last was over a page (B25 T-209).
    @Published var contentClip: ContentClip?
    var contentPointer: ContentPointer?
    /// Whether the Pages column is open — `ViewerViewModel+Navigation.swift`.
    /// Open from the start, as on Linux and Windows.
    @Published var showsPages = true
    /// One tools column, with navigation independent of the document's modes.
    @Published var showsTools = true
    @Published var toolsTab: ToolsTab = .annotate
    /// Add Text is armed: the next click on a page places a new line there.
    @Published var textInsertArmed = false
    /// Where the new line goes, while its sheet is up.
    @Published var pendingTextInsert: PendingTextInsert?
    /// Add Image is armed with these bytes: the next click places them.
    @Published var imageInsertBytes: Data?
    /// The Organize card being dragged, if any.
    var organizePageDrag: OrganizePageDrag?
    /// Organize opens on Documents, as on Linux and Windows; off shows Pages
    /// — `ViewerViewModel+OrganizeDocuments.swift`.
    @Published var organizeShowsDocuments = true
    /// The Documents card being dragged, if any.
    var organizeBlockDrag: OrganizePageDrag?
    /// Add PDFs is reading and preparing sources — `ViewerViewModel+Import.swift`.
    @Published var isImporting = false
    /// How many picked PDFs are prepared so far, while `isImporting`.
    @Published var importProgress: ImportProgress?
    /// Bumped by every pick and every Cancel import, so a preparation that
    /// finishes after its run was cancelled is dropped.
    var importRun = 0
    let importPrompts: ImportPrompts
    /// Home's Recent list, its page-1 previews, and where it is kept —
    /// `ViewerViewModel+Recents.swift`.
    @Published var recentDocuments: [RecentDocument] = []
    @Published var recentPreviews: [String: RecentPreview] = [:]
    /// Paths whose preview is queued, so a list change does not queue it twice.
    var recentPreviewsRequested = Set<String>()
    let recentStore: RecentDocumentStore
    /// Serial and never cancelled, unlike `operationQueue`: an open must not
    /// drop a pending write of the list.
    let recentStorageQueue: OperationQueue = {
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 1
        return queue
    }()
    /// The file the document being opened came from, recorded in Recent once
    /// it loads; `nil` for samples, new documents and anything not a file.
    var pendingRecentPath: String?
    /// A Home tool picked with no document open: applied once the document
    /// chosen for it loads — `ViewerViewModel+HomeTools.swift`.
    var pendingHomeTool: HomeTool?
    /// Home is showing over an open document, which stays as it was —
    /// `ViewerViewModel+HomeTools.swift`. A document that loads closes it.
    @Published var showsHome = false
    /// Home/Recent navigation only; the open document's modes remain intact.
    @Published var homeSection: ShellSection = .home
    @Published var recentFocusRequest: UInt = 0
    /// Share's signature question and picker — `ViewerViewModel+Share.swift`.
    let sharePrompts: SharePrompts
    let store: ViewerStore
    /// Serial: renders, searches, character fetches and saves all go through
    /// it. Not `private`: the `ViewerViewModel+…` extensions enqueue on it.
    let operationQueue: OperationQueue
    let prompts: SavePrompts
    let signingPrompts: SigningPrompts
    /// Whether File ▸ Sign with Card or Token… and its Fill & Sign button are
    /// offered — `ViewerViewModel+TokenSigning.swift` says why the sandboxed
    /// build leaves them out.
    let offersTokenSigning: Bool
    private let sampleLoader: () throws -> Data
    private var subscriptions = Set<AnyCancellable>()
    /// The pending viewport size report — see `ViewerViewModel+Zoom.swift`.
    var pendingViewport: DispatchWorkItem?
    /// Each page's queued render — `ViewerViewModel+PageWindow.swift`.
    var renderOperations: [Int: (generation: UInt, operation: Operation)] = [:]
    /// Draw Signature's sheet, and where the remembered signature lives —
    /// `ViewerViewModel+Signature.swift`.
    @Published var signatureSheet: SignatureSheet?
    let signatures: SignatureStore
    /// The Delete-key monitor — `ViewerViewModel+Annotations.swift`.
    var annotationKeyMonitor: Any?

    init(
        store: ViewerStore = ViewerStore(client: UniFfiPdfCoreClient()),
        prompts: SavePrompts = .modal,
        signingPrompts: SigningPrompts = .modal,
        importPrompts: ImportPrompts = .modal,
        recentStore: RecentDocumentStore = InMemoryRecentDocumentStore(),
        sharePrompts: SharePrompts = .modal,
        signatures: SignatureStore = FileSignatureStore(url: FileSignatureStore.defaultURL),
        offersTokenSigning: Bool = !AppSandbox.isActive,
        sampleLoader: @escaping () throws -> Data = ViewerViewModel.loadBundledSample
    ) {
        self.store = store
        self.prompts = prompts
        self.signingPrompts = signingPrompts
        self.importPrompts = importPrompts
        self.recentStore = recentStore
        self.sharePrompts = sharePrompts
        self.signatures = signatures
        self.offersTokenSigning = offersTokenSigning
        self.sampleLoader = sampleLoader
        operationQueue = OperationQueue()
        operationQueue.maxConcurrentOperationCount = 1
        // Linux and Windows open every document fitted to the window width.
        store.setZoomMode(.fitWidth)
        // No `receive(on:)` hop: the store already publishes on main, and
        // `@Published` fires on `willSet` — so a hop would be the only reason
        // the sink ever saw the *new* state. Use the emitted value instead of
        // reading `store.state` back, and the update stays synchronous.
        store.$state
            .sink { [weak self] state in
                self?.title = Self.windowTitle(for: state)
                self?.recordPendingRecent(state)
                self?.applyPendingHomeTool(state)
                if case .loaded = state { self?.showsHome = false }
            }
            .store(in: &subscriptions)
        loadRecentDocuments()
        // `ViewerRootView` observes only this view model (`@ObservedObject var
        // model`), not `store` directly. `store` is its own `ObservableObject`
        // with its own `@Published` properties (`pageSlots`, `zoom`), so a page
        // finishing its render — or a zoom change — fires `store`'s
        // `objectWillChange`, not this object's. Without forwarding it here,
        // SwiftUI never re-evaluates the view after the first paint: every
        // `PageView` stays frozen showing "Rendering page N…" forever, even
        // though the store already moved the slot to `.rendered`.
        store.objectWillChange
            .sink { [weak self] in self?.objectWillChange.send() }
            .store(in: &subscriptions)
    }

    func selectDocument() {
        proceedPastUnsavedChanges { [weak self] in
            guard let self else { return }
            let panel = NSOpenPanel()
            panel.allowedContentTypes = [.pdf]
            panel.allowsMultipleSelection = false
            guard panel.runModal() == .OK, let url = panel.url else {
                self.pendingHomeTool = nil
                self.store.selectionCancelled()
                return
            }
            self.open(url: url)
        }
    }

    /// File ▸ New: a blank, unsaved A4 document, after the usual question
    /// about unsaved changes — named as Windows names it.
    func newDocument() {
        proceedPastUnsavedChanges { [weak self] in
            guard let self else { return }
            self.operationQueue.cancelAllOperations()
            // Like every other open, a new document clears a drop's notice.
            self.notice = ""
            self.documentName = "Untitled"
            self.pendingRecentPath = nil
            self.store.openBlank()
        }
    }

    func open(url: URL) {
        loadAndOpen(name: url.lastPathComponent, recentPath: url.path) { try Data(contentsOf: url) }
    }

    /// Opens the sample document bundled with the app (see the Resources
    /// build phase in `Vitela.xcodeproj`), so a fresh install has something
    /// to render without the user supplying a PDF first.
    func openSample() {
        proceedPastUnsavedChanges { [weak self] in
            guard let self else { return }
            self.loadAndOpen(name: "vitela-sample.pdf", self.sampleLoader)
        }
    }

    /// Opens the AES-128 encrypted sample bundled alongside the plain one, to
    /// exercise the password prompt. User password: `user-aes-pass` (see
    /// `tests/fixtures/README.md`).
    func openAes128Sample() {
        openBundledResource(named: "aes_128_user_and_owner")
    }

    /// Opens the RC4-128 encrypted sample. User password: `user-rc4-pass`.
    func openRc4128Sample() {
        openBundledResource(named: "rc4_128_user_and_owner")
    }

    private func openBundledResource(named name: String) {
        proceedPastUnsavedChanges { [weak self] in
            self?.loadAndOpen(name: "\(name).pdf") { try Self.loadBundledResource(named: name) }
        }
    }

    /// Not `private`: Recent opens its cards through it, inside their access.
    func loadAndOpen(name: String, recentPath: String? = nil, _ load: @escaping () throws -> Data) {
        operationQueue.cancelAllOperations()
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            do {
                let bytes = try load()
                DispatchQueue.main.async {
                    self.notice = ""
                    self.documentName = name
                    self.isOrganizing = false
                    self.pendingRecentPath = recentPath
                    self.store.open(bytes: bytes)
                }
            } catch {
                // A disk-read failure is not a malformed PDF. Reporting it as
                // one used to discard the real reason (permissions, missing
                // file) behind a generic parse error.
                let failure = ViewerFailure.readFailed(error.localizedDescription)
                DispatchQueue.main.async { self.store.reportOpenFailure(failure) }
            }
        }
    }

    /// `scale` is the screen's pixels per point where the page is shown;
    /// `nil` keeps the last one.
    func render(page: Int, scale: Double? = nil) {
        if let scale { store.setRenderScale(scale) }
        enqueue(store.beginRender(page: page, force: renderWasDropped(page)))
        // Piggybacks on the render queue rather than its own trigger: a page
        // becomes selectable at roughly the same time its bitmap appears,
        // and `beginPageCharactersFetch` is already a no-op once cached.
        enqueueCharacters(store.beginPageCharactersFetch(page: page))
    }

    func retry(page: Int) {
        enqueue(store.retry(page: page))
    }

    // MARK: - Search

    func runSearch(_ query: String) {
        guard let request = store.beginSearch(query: query) else {
            if query.isEmpty { store.clearSearch() }
            return
        }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let result = self.store.searchResult(for: request)
            DispatchQueue.main.async { self.store.applySearch(result, for: request) }
        }
    }

    func stepSearchMatch(by delta: Int) {
        store.stepSearchMatch(by: delta)
    }

    // MARK: - Selection

    func beginSelection(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        let point = Self.pdfPoint(from: location, dimensions: dimensions, zoom: zoom)
        store.beginSelection(page: page, xPt: point.x, yPt: point.y)
    }

    func extendSelection(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        let point = Self.pdfPoint(from: location, dimensions: dimensions, zoom: zoom)
        store.extendSelection(page: page, xPt: point.x, yPt: point.y)
    }

    /// Copies the current selection to the general pasteboard. A no-op when
    /// there is nothing selected, so wiring this straight to Cmd+C is safe.
    func copySelection() {
        // Asked again here, though a refused document never selects: this
        // is the call that hands the text over.
        if let refusal = store.textExtractionRefusal {
            store.editStatus = refusal
            return
        }
        guard let text = store.selectedText else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func enqueueCharacters(_ request: ViewerStore.PageCharactersRequest?) {
        guard let request else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let characters = self.store.pageCharactersResult(for: request)
            DispatchQueue.main.async { self.store.applyPageCharacters(characters, for: request) }
        }
    }

    /// View space (top-left origin, in points already — SwiftUI coordinates
    /// aren't scaled by the screen's backing factor) to PDF space
    /// (bottom-left origin, unzoomed).
    private static func pdfPoint(from location: CGPoint, dimensions: PageDimensions, zoom: Double) -> (x: Double, y: Double) {
        (Double(location.x) / zoom, dimensions.height - Double(location.y) / zoom)
    }

    /// Reopens the document last selected, this time with a password the
    /// user just typed into the prompt.
    func submitPassword(_ password: String) {
        store.retryPassword(password)
    }

    private func enqueue(_ request: RenderRequest?) {
        guard let request else { return }
        let operation = BlockOperation { [weak self] in
            guard let self else { return }
            // `renderResult` only reads the request and the immutable client;
            // every mutation of the store happens back on main.
            let result = self.store.renderResult(for: request)
            DispatchQueue.main.async { self.store.applyRender(result, for: request) }
        }
        renderOperations[request.page] = (request.generation, operation)
        operationQueue.addOperation(operation)
    }

    /// Reads `assets/sample/vitela-sample.pdf`, copied into the app bundle by
    /// the "Sample document" Resources build phase under the same file name
    /// every shell uses (see assets/README.md).
    private static func loadBundledSample() throws -> Data {
        try loadBundledResource(named: "vitela-sample")
    }

    /// Reads one of the PDFs copied into the app bundle by the Resources
    /// build phase, by resource name (without the `.pdf` extension).
    private static func loadBundledResource(named name: String) throws -> Data {
        guard let url = Bundle.main.url(forResource: name, withExtension: "pdf") else {
            throw ViewerFailure.readFailed("The bundled sample document is missing.")
        }
        return try Data(contentsOf: url)
    }

    /// Not named `title(for:)`: that would collide with the `title` property
    /// when referenced as `ViewerViewModel.title`.
    private static func windowTitle(for state: ViewerState) -> String {
        switch state {
        case .empty: return "Open a PDF to begin"
        case .loading: return "Opening PDF…"
        case .loaded: return "PDF open"
        case .error: return "Could not open PDF"
        }
    }
}
