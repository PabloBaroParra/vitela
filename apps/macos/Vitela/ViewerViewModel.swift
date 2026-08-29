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
        guard let url = Bundle.main.url(forResource: "vitela-sample", withExtension: "pdf") else {
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
