// Page thumbnails for the Organize grid. Separate from `pageSlots` on
// purpose: a thumbnail is a fixed-size picture, not the page at the reader's
// zoom, and rendering one must not make the reader's bitmap stale. Requests
// reuse `RenderRequest`/`renderResult(for:)`, so they run on the same queue
// and are dropped the same way when the layout changes underneath them.
import Foundation

extension ViewerStore {
    /// The longer side of a thumbnail, in view points — Windows' card box.
    static let thumbnailBox = 150.0

    /// The DPI that fits a page's longer side into the thumbnail box at a
    /// display's backing scale — Windows' `ThumbnailDpi`.
    static func thumbnailDpi(for page: PageDimensions, scale: Double) -> Int {
        let longerSide = max(page.width, page.height, 1)
        let dpi = (thumbnailBox * max(scale, 1) * 72 / longerSide).rounded(.up)
        return Int(min(max(dpi, 8), 300))
    }

    /// Issues the render for `page`'s thumbnail, or `nil` when it is already
    /// shown or on its way.
    func beginThumbnail(page: Int, scale: Double) -> RenderRequest? {
        guard let document, pageSlots.indices.contains(page),
              thumbnails[page] == nil, !thumbnailsRequested.contains(page)
        else { return nil }
        thumbnailsRequested.insert(page)
        return RenderRequest(
            generation: generation,
            page: page,
            document: document,
            dpi: Self.thumbnailDpi(for: pageSlots[page].dimensions, scale: scale),
            zoom: 0
        )
    }

    /// A failed thumbnail stays requested, so scrolling past it does not
    /// restart the same doomed render; the card shows its number alone.
    func applyThumbnail(_ result: Result<RenderedPage, ViewerFailure>, for request: RenderRequest) {
        guard request.generation == generation, case let .success(image) = result, isDisplayable(image) else { return }
        thumbnails[request.page] = image
    }
}
