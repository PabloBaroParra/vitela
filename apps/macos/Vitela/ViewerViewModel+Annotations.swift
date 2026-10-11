import Foundation

/// Routes a drag on a page: to the armed annotation tool when there is one,
/// otherwise to text selection — Windows' `ConnectAnnotationPointer` order,
/// where selection is tried only once annotations refuse the gesture.
extension ViewerViewModel {
    func pageDragBegan(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        if let point = annotationPoint(page: page, location: location, zoom: zoom),
           store.beginAnnotationDraft(page: page, at: point) {
            return
        }
        beginSelection(page: page, location: location, dimensions: dimensions, zoom: zoom)
    }

    func pageDragChanged(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        if store.annotationDraft != nil {
            if let point = annotationPoint(page: page, location: location, zoom: zoom) {
                store.continueAnnotationDraft(page: page, to: point)
            }
            return
        }
        extendSelection(page: page, location: location, dimensions: dimensions, zoom: zoom)
    }

    func pageDragEnded(page: Int, location: CGPoint, zoom: Double) {
        guard store.annotationDraft != nil else { return }
        guard let point = annotationPoint(page: page, location: location, zoom: zoom) else {
            return store.cancelAnnotationDraft()
        }
        store.endAnnotationDraft(page: page, at: point)
    }

    /// A pointer position on the drawn page in page space, through the
    /// core's turn-aware placement.
    private func annotationPoint(page: Int, location: CGPoint, zoom: Double) -> PdfPoint? {
        guard let placement = store.placement(ofPage: page, zoom: zoom) else { return nil }
        return store.client.pointToPdf(x: Double(location.x), y: Double(location.y), on: placement)
    }
}
