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
    /// from the store's zoom is stale and will be re-rendered when it is next
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

/// Main-thread only, except for `renderResult(for:)`, which is explicitly
/// documented as thread-safe. Every stored property is mutated on main.
final class ViewerStore: ObservableObject {
    static let minimumZoom = 0.5
    static let maximumZoom = 4.0
    static let baseDpi = 72.0

    @Published private(set) var state: ViewerState = .empty
    @Published private(set) var pageSlots: [PageSlot] = []
    @Published private(set) var zoom = 1.0

    private let client: PdfCoreClient
    private var document: (any PdfDocument)?
    private var generation: UInt = 0
    /// The bytes behind the most recent `open` attempt, kept around so a
    /// password prompt can retry without asking the user to re-pick the file.
    private var pendingBytes: Data?

    init(client: PdfCoreClient) {
        self.client = client
    }

    func selectionCancelled() {}

    func open(bytes: Data, password: String? = nil) {
        generation += 1
        let attemptedGeneration = generation
        pendingBytes = bytes
        state = .loading
        do {
            let opened = try client.open(bytes: bytes, password: password)
            guard attemptedGeneration == generation else { return }
            document = opened
            pageSlots = opened.pages.enumerated().map { PageSlot(index: $0.offset, dimensions: $0.element) }
            state = .loaded
        } catch let failure as ViewerFailure {
            guard attemptedGeneration == generation else { return }
            state = .error(failure)
        } catch {
            guard attemptedGeneration == generation else { return }
            state = .error(.openFailed(String(describing: error)))
        }
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

    func setZoom(_ value: Double) {
        zoom = min(max(value, Self.minimumZoom), Self.maximumZoom)
    }

    /// Issues render work for `page`, or returns `nil` when there is nothing to
    /// do — the page is already rendered (or in flight, or failed) at the
    /// current zoom. `force` bypasses that check for an explicit retry.
    func beginRender(page: Int, force: Bool = false) -> RenderRequest? {
        guard pageSlots.indices.contains(page), let document else { return nil }
        guard force || !isUpToDate(pageSlots[page]) else { return nil }

        pageSlots[page].status = .rendering
        pageSlots[page].renderZoom = zoom
        return RenderRequest(
            generation: generation,
            page: page,
            document: document,
            dpi: dpi(for: zoom),
            zoom: zoom
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

    private func isUpToDate(_ slot: PageSlot) -> Bool {
        guard slot.renderZoom == zoom else { return false }
        switch slot.status {
        case .rendering, .rendered, .failed: return true
        case .idle: return false
        }
    }

    /// Core Graphics reads `stride * height` bytes out of the provider without
    /// bounds-checking, so the buffer length has to be verified here — a short
    /// buffer is an out-of-bounds read, not a blank page.
    private func isDisplayable(_ image: RenderedPage) -> Bool {
        image.width > 0
            && image.height > 0
            && image.stride >= image.width * 4
            && image.rgba.count >= image.stride * image.height
    }
}
