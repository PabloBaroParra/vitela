import Foundation

/// Edit Text on macOS: the toolbar toggle, reading each page's content on
/// the operation queue, a click opening the line under it in a sheet, a drag
/// moving it, and the sheet's Save/Delete running as page work. The rules are the store's
/// (`ViewerStore+ContentEdit.swift`).
extension ViewerViewModel {
    /// Not over Organize, which hides the pages an edit would redraw.
    var canEditContent: Bool { store.document != nil && !isOrganizing && !store.pageWorkInFlight }

    func setContentEditMode(_ on: Bool) {
        guard !on || canEditContent else { return }
        store.setContentEditMode(on)
        if !on { contentTextEditor = nil }
    }

    /// Reads `page`'s content once, while the mode is on.
    func readPageContent(page: Int) {
        guard let request = store.beginPageContent(page: page) else { return }
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let result = self.store.pageContentResult(for: request)
            DispatchQueue.main.async { self.store.applyPageContent(result, for: request) }
        }
    }

    /// A press on a page. Claims it whenever the mode is on, so a miss
    /// neither selects text nor an annotation.
    func contentPress(page: Int, location: CGPoint, zoom: Double) -> Bool {
        guard store.contentEditMode else { return false }
        guard store.pageContent[page] != nil else {
            readPageContent(page: page)
            store.editStatus = "Reading the page’s text… click again in a moment."
            return true
        }
        guard let point = annotationPoint(page: page, location: location, zoom: zoom),
              store.beginContentDrag(page: page, at: point, zoom: zoom) != nil
        else {
            store.editStatus = ContentEditing.missed
            return true
        }
        return true
    }

    func contentDragged(page: Int, location: CGPoint, zoom: Double) {
        guard let point = annotationPoint(page: page, location: location, zoom: zoom) else { return }
        store.continueContentDrag(to: point)
    }

    /// The press ends: a click opens the line, a drag moves it.
    func contentReleased(page: Int, location: CGPoint, zoom: Double) {
        guard store.contentDrag != nil else { return }
        guard let point = annotationPoint(page: page, location: location, zoom: zoom) else {
            return store.cancelContentDrag()
        }
        switch store.endContentDrag(at: point) {
        case let .open(run):
            if run.substitutesFont { store.editStatus = ContentEditing.fontSubstituted }
            contentTextEditor = run
        case let .move(request):
            enqueuePageWork(request)
        case .nothing:
            break
        }
    }

    func retypeText(_ run: ContentTextRun, to text: String) {
        contentTextEditor = nil
        enqueuePageWork(store.beginContentEdit(.retypeText(run, to: text)))
    }

    func deleteText(_ run: ContentTextRun) {
        contentTextEditor = nil
        enqueuePageWork(store.beginContentEdit(.removeText(run)))
    }

    func cancelTextEdit() {
        contentTextEditor = nil
        store.editStatus = "Edit cancelled."
    }
}
