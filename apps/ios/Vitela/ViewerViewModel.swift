import Combine
import Foundation

/// The iOS half of the shell. The store it drives is shared with macOS
/// (apps/apple/Shared/ViewerStore.swift); what differs, and the only reason
/// this type is not shared too, is how a document gets picked: macOS runs a
/// modal `NSOpenPanel`, iOS presents a document picker from the view and hands
/// the result back here.
final class ViewerViewModel: ObservableObject {
    @Published private(set) var title = ViewerViewModel.screenTitle(for: .empty)
    /// Drives the view's `.fileImporter`. Two-way on purpose: SwiftUI writes
    /// `false` back when the picker is dismissed by a gesture rather than by a
    /// selection, and that dismissal must not leave the flag stuck on.
    @Published var isSelectingDocument = false

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
            .sink { [weak self] state in self?.title = Self.screenTitle(for: state) }
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
        isSelectingDocument = true
    }

    /// Called by the view with whatever `.fileImporter` produced.
    func finishSelection(_ result: Result<[URL], Error>) {
        isSelectingDocument = false
        switch result {
        case let .success(urls):
            guard let url = urls.first else {
                store.selectionCancelled()
                return
            }
            open(url: url)
        case let .failure(error):
            // A dismissed picker arrives here as a failure, not as an empty
            // success. Reporting that as a read error would put the viewer in
            // an error state just because the user changed their mind.
            guard (error as NSError).code != NSUserCancelledError else {
                store.selectionCancelled()
                return
            }
            store.reportOpenFailure(.readFailed(error.localizedDescription))
        }
    }

    func open(url: URL) {
        loadAndOpen {
            // The picker hands back a file outside the app's container. Without
            // claiming the security-scoped resource first, the read fails with
            // a permissions error that reads like a missing file — and unlike
            // macOS, on iOS there is no fallback path that would still work.
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return try Data(contentsOf: url)
        }
    }

    /// Opens the sample document bundled with the app (see the Resources
    /// build phase in `VitelaIOS.xcodeproj`), so a fresh install has something
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
                // one would discard the real reason (permissions, missing file)
                // behind a generic parse error.
                let failure = ViewerFailure.readFailed(error.localizedDescription)
                DispatchQueue.main.async { self.store.reportOpenFailure(failure) }
            }
        }
    }

    func render(page: Int) {
        enqueue(store.beginRender(page: page))
    }

    func retry(page: Int) {
        enqueue(store.retry(page: page))
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
    private static func screenTitle(for state: ViewerState) -> String {
        switch state {
        case .empty: return "Open a PDF to begin"
        case .loading: return "Opening PDF…"
        case .loaded: return "PDF open"
        case .error: return "Could not open PDF"
        }
    }
}
