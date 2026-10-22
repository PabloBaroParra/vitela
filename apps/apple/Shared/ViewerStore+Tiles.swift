// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// A rectangle of a page's pixel grid at one DPI — the core's `FfiRenderTile`.
struct PixelRect: Hashable {
    let left: Int
    let top: Int
    let width: Int
    let height: Int
}

/// The part of a page in view, in points from the drawn page's top-left.
struct PageViewRect: Equatable {
    let x: Double
    let y: Double
    let width: Double
    let height: Double
}

/// The sharp tiles drawn over one page's bitmap, all at `dpi`.
struct PageTiles {
    let dpi: Int
    var images: [PixelRect: RenderedPage] = [:]
    var pending: Set<PixelRect> = []
}

/// Tile work in flight. Like `RenderRequest`, it carries everything the
/// background queue needs, so `tilesResult(for:)` never reads the store.
struct TileRequest {
    let generation: UInt
    let page: Int
    let document: any PdfDocument
    let dpi: Int
    let tiles: [PixelRect]
}

/// Viewport tiles — Linux's `layout.rs` `tile_plan` and Windows'
/// `ViewportTilePlan`. When the screen asks for more DPI than the page
/// bitmap's pixel budget allows (`ViewerStore+RenderDpi.swift`), the part of
/// the page in view is rendered again at the full DPI, in tiles of a fixed
/// grid anchored to the page — never to the scroll offset, so scrolling
/// inside the same tiles re-renders nothing. The page bitmap stays beneath
/// as the fallback while tiles arrive, or if they fail.
extension ViewerStore {
    /// Linux's `TILE_EDGE_PX` and Windows' `TileEdgePixels`.
    static let tileEdge = 1024

    /// The DPI and the tiles of the grid that cover `visible` on `page`, or
    /// `nil` when the page bitmap is already as sharp as the screen asks.
    func tilePlan(page: Int, zoom: Double, visible: PageViewRect) -> (dpi: Int, tiles: [PixelRect])? {
        guard pageSlots.indices.contains(page) else { return nil }
        let intended = intendedDpi(zoom: zoom)
        guard Int(intended) > dpi(forPage: page, zoom: zoom) else { return nil }
        let size = pageSlots[page].dimensions
        let pageWidth = Int((size.width * intended / Self.baseDpi).rounded(.up))
        let pageHeight = Int((size.height * intended / Self.baseDpi).rounded(.up))
        let perPoint = intended / Self.baseDpi
        let left = min(max(Int((visible.x * perPoint).rounded(.down)), 0), pageWidth)
        let top = min(max(Int((visible.y * perPoint).rounded(.down)), 0), pageHeight)
        let right = min(max(Int(((visible.x + visible.width) * perPoint).rounded(.up)), left), pageWidth)
        let bottom = min(max(Int(((visible.y + visible.height) * perPoint).rounded(.up)), top), pageHeight)
        let edge = Self.tileEdge
        var tiles: [PixelRect] = []
        for y in stride(from: top / edge * edge, to: bottom, by: edge) {
            for x in stride(from: left / edge * edge, to: right, by: edge) {
                tiles.append(PixelRect(left: x, top: y, width: min(edge, pageWidth - x), height: min(edge, pageHeight - y)))
            }
        }
        return (Int(intended), tiles)
    }

    /// Asks for the tiles covering `visible` that are neither drawn nor on
    /// their way, and forgets drawn ones that left the view. `nil` when
    /// nothing is missing, or when the page needs no tiles at this zoom.
    func beginTiles(page: Int, visible: PageViewRect) -> TileRequest? {
        guard let document, let plan = tilePlan(page: page, zoom: zoom(forPage: page), visible: visible) else {
            forgetTiles(page: page)
            return nil
        }
        var layer = pageTiles[page].flatMap { $0.dpi == plan.dpi ? $0 : nil } ?? PageTiles(dpi: plan.dpi)
        let wanted = Set(plan.tiles)
        layer.images = layer.images.filter { wanted.contains($0.key) }
        let missing = plan.tiles.filter { layer.images[$0] == nil && !layer.pending.contains($0) }
        layer.pending.formUnion(missing)
        // Compared by shape, not by pixels: this runs on every scroll step.
        let current = pageTiles[page]
        if current?.dpi != layer.dpi || current?.pending != layer.pending
            || current.map({ Set($0.images.keys) }) != Set(layer.images.keys) {
            pageTiles[page] = layer
        }
        guard !missing.isEmpty else { return nil }
        return TileRequest(generation: generation, page: page, document: document, dpi: plan.dpi, tiles: missing)
    }

    /// Thread-safe, like `renderResult(for:)`.
    func tilesResult(for request: TileRequest) -> Result<[RenderedPage], ViewerFailure> {
        do {
            return .success(try client.renderTiles(document: request.document, page: request.page, dpi: request.dpi, tiles: request.tiles))
        } catch let failure as ViewerFailure {
            return .failure(failure)
        } catch {
            return .failure(.renderFailed(page: request.page, message: String(describing: error)))
        }
    }

    /// Lands tiles on their page if it is still tiled at their DPI. A failed
    /// batch leaves the page bitmap showing; the next scroll asks again.
    func applyTiles(_ result: Result<[RenderedPage], ViewerFailure>, for request: TileRequest) {
        guard request.generation == generation, var layer = pageTiles[request.page], layer.dpi == request.dpi else { return }
        layer.pending.subtract(request.tiles)
        if case let .success(images) = result, images.count == request.tiles.count {
            for (tile, image) in zip(request.tiles, images) where isDisplayable(image) {
                layer.images[tile] = image
            }
        }
        pageTiles[request.page] = layer
    }

    /// A page that scrolled out of view keeps no tiles.
    func forgetTiles(page: Int) {
        // Only when there is something to forget: an assignment publishes,
        // and an untiled page reports its view rect on every scroll step.
        if pageTiles[page] != nil { pageTiles[page] = nil }
    }
}
