import SwiftUI

/// The sharp tiles laid over a page's bitmap where it is shown past the
/// bitmap's pixel budget (`../apple/Shared/ViewerStore+Tiles.swift`).
/// Each edge is snapped to the screen's pixel grid from the tile's own
/// pixel edge, so neighbours share an exact edge — Linux's `tile_placement`.
struct PageTilesView: View {
    let tiles: PageTiles?
    let zoom: Double
    @Environment(\.displayScale) private var displayScale

    var body: some View {
        ZStack(alignment: .topLeading) {
            if let tiles {
                ForEach(Array(tiles.images.keys), id: \.self) { tile in
                    if let image = tiles.images[tile]?.nsImage {
                        let frame = placement(tile, dpi: tiles.dpi)
                        Image(nsImage: image)
                            .resizable()
                            .frame(width: frame.width, height: frame.height)
                            .offset(x: frame.minX, y: frame.minY)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func placement(_ tile: PixelRect, dpi: Int) -> CGRect {
        let pointsPerPixel = ViewerStore.baseDpi * zoom / Double(dpi)
        let scale = max(Double(displayScale), 1)
        func edge(_ pixels: Int) -> Double { (Double(pixels) * pointsPerPixel * scale).rounded() / scale }
        let left = edge(tile.left), top = edge(tile.top)
        return CGRect(x: left, y: top, width: edge(tile.left + tile.width) - left, height: edge(tile.top + tile.height) - top)
    }
}

/// Reports the part of a page inside the scroll view, in page points, as it
/// changes — what `ViewerViewModel.renderTiles` plans tiles from.
struct PageViewRectReader: View {
    let space: String
    let viewport: CGSize
    let zoom: Double
    let changed: (PageViewRect) -> Void

    var body: some View {
        GeometryReader { geometry in
            let rect = visible(geometry.frame(in: .named(space)))
            Color.clear
                .onAppear { changed(rect) }
                .onChange(of: rect, perform: changed)
        }
    }

    private func visible(_ page: CGRect) -> PageViewRect {
        let shown = page.intersection(CGRect(origin: .zero, size: viewport))
        guard !shown.isNull, zoom > 0 else { return PageViewRect(x: 0, y: 0, width: 0, height: 0) }
        return PageViewRect(
            x: Double(shown.minX - page.minX) / zoom,
            y: Double(shown.minY - page.minY) / zoom,
            width: Double(shown.width) / zoom,
            height: Double(shown.height) / zoom
        )
    }
}
