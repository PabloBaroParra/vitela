import AppKit
import Combine
import Foundation
import UniformTypeIdentifiers

final class ViewerViewModel: ObservableObject {
    @Published private(set) var title = ViewerViewModel.windowTitle(for: .empty)
    let store: ViewerStore
    private let operationQueue: OperationQueue
    private let sampleLoader: () throws -> Data
    private var subscriptions = Set<AnyCancellable>()

    init(
        store: ViewerStore = ViewerStore(client: UniFfiPdfCoreClient()),
        sampleLoader: @escaping () throws -> Data = ViewerViewModel.loadBundledSample
    ) {
        self.store = store
        self.sampleLoader = sampleLoader
        operationQueue = OperationQueue()
        operationQueue.maxConcurrentOperationCount = 1
        // No `receive(on:)` hop: the store already publishes on main, and
        // `@Published` fires on `willSet` — so a hop would be the only reason
        // the sink ever saw the *new* state. Use the emitted value instead of
        // reading `store.state` back, and the update stays synchronous.
        store.$state
            .sink { [weak self] state in self?.title = Self.windowTitle(for: state) }
            .store(in: &subscriptions)
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
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.pdf]
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else {
            store.selectionCancelled()
            return
        }
        open(url: url)
    }

    func open(url: URL) {
        loadAndOpen { try Data(contentsOf: url) }
    }

    /// Opens the sample document bundled with the app (see the Resources
    /// build phase in `Vitela.xcodeproj`), so a fresh install has something
    /// to render without the user supplying a PDF first.
    func openSample() {
        loadAndOpen(sampleLoader)
    }

    /// Opens the AES-128 encrypted sample bundled alongside the plain one, to
    /// exercise the password prompt. User password: `user-aes-pass` (see
    /// `tests/fixtures/README.md`).
    func openAes128Sample() {
        loadAndOpen { try Self.loadBundledResource(named: "aes_128_user_and_owner") }
    }

    /// Opens the RC4-128 encrypted sample. User password: `user-rc4-pass`.
    func openRc4128Sample() {
        loadAndOpen { try Self.loadBundledResource(named: "rc4_128_user_and_owner") }
    }

    private func loadAndOpen(_ load: @escaping () throws -> Data) {
        operationQueue.cancelAllOperations()
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            do {
                let bytes = try load()
                DispatchQueue.main.async { self.store.open(bytes: bytes) }
            } catch {
                // A disk-read failure is not a malformed PDF. Reporting it as
                // one used to discard the real reason (permissions, missing
                // file) behind a generic parse error.
                let failure = ViewerFailure.readFailed(error.localizedDescription)
                DispatchQueue.main.async { self.store.reportOpenFailure(failure) }
            }
        }
    }

    func render(page: Int) {
        enqueue(store.beginRender(page: page))
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
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            // `renderResult` only reads the request and the immutable client;
            // every mutation of the store happens back on main.
            let result = self.store.renderResult(for: request)
            DispatchQueue.main.async { self.store.applyRender(result, for: request) }
        }
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
