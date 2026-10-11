import AppKit
import Foundation

/// Routes a drag on a page: to the armed annotation tool when there is one,
/// then to a placed annotation under the pointer (select, move, resize),
/// and only then to text selection — Windows' `ConnectAnnotationPointer`
/// order, where selection is tried only once annotations refuse the gesture.
extension ViewerViewModel {
    func pageDragBegan(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        if let point = annotationPoint(page: page, location: location, zoom: zoom),
           store.beginFieldDraft(page: page, at: point)
           || store.beginFieldGesture(page: page, at: point, zoom: zoom)
           || store.beginAnnotationDraft(page: page, at: point)
           || store.beginAnnotationGesture(page: page, at: point, zoom: zoom) {
            return
        }
        beginSelection(page: page, location: location, dimensions: dimensions, zoom: zoom)
    }

    func pageDragChanged(page: Int, location: CGPoint, dimensions: PageDimensions, zoom: Double) {
        if store.fieldDraft != nil {
            if let point = annotationPoint(page: page, location: location, zoom: zoom) {
                store.continueFieldDraft(page: page, to: point)
            }
            return
        }
        if store.fieldGesture != nil {
            if let point = annotationPoint(page: page, location: location, zoom: zoom) {
                store.continueFieldGesture(to: point)
            }
            return
        }
        if store.formEditMode { return }
        if store.annotationDraft != nil {
            if let point = annotationPoint(page: page, location: location, zoom: zoom) {
                store.continueAnnotationDraft(page: page, to: point)
            }
            return
        }
        if store.annotationGesture != nil {
            if let point = annotationPoint(page: page, location: location, zoom: zoom) {
                store.continueAnnotationGesture(to: point)
            }
            return
        }
        extendSelection(page: page, location: location, dimensions: dimensions, zoom: zoom)
    }

    func pageDragEnded(page: Int, location: CGPoint, zoom: Double) {
        if store.fieldDraft != nil {
            let point = annotationPoint(page: page, location: location, zoom: zoom)
            return enqueuePageWork(point.flatMap { store.endFieldDraft(page: page, at: $0) })
        }
        if store.fieldGesture != nil {
            let point = annotationPoint(page: page, location: location, zoom: zoom)
            if point == nil { store.fieldGesture = nil }
            return enqueuePageWork(point.flatMap { store.endFieldGesture(at: $0) })
        }
        if store.annotationGesture != nil {
            guard let point = annotationPoint(page: page, location: location, zoom: zoom) else {
                return store.cancelAnnotationGesture()
            }
            return store.endAnnotationGesture(at: point)
        }
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

extension ViewerViewModel {
    /// Delete and Backspace remove the selected annotation — Linux's
    /// `win.delete-annotation` accel — or, in Edit fields mode, the selected
    /// field (Windows' `DeleteFormField_Invoked`), but never while a text field is
    /// editing, where they belong to the text (Windows' `TextInputHasFocus`).
    /// A local monitor rather than a button shortcut, which would take the
    /// key from every text field in the window.
    func installAnnotationKeyMonitor() {
        guard annotationKeyMonitor == nil else { return }
        annotationKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, Self.isDeleteKey(event), !(event.window?.firstResponder is NSText) else { return event }
            if self.store.formEditMode, self.store.selectedField != nil {
                self.enqueuePageWork(self.store.beginFieldDelete())
                return nil
            }
            guard self.store.selectedAnnotation != nil else { return event }
            self.store.deleteSelectedAnnotation()
            return nil
        }
    }

    /// Backspace (51) and forward delete (117), with no modifiers.
    static func isDeleteKey(_ event: NSEvent) -> Bool {
        (event.keyCode == 51 || event.keyCode == 117)
            && event.modifierFlags.intersection(.deviceIndependentFlagsMask).isEmpty
    }
}
