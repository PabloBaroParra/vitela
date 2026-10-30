import Foundation

/// A click that placed Add Text: where the new line's top-left corner goes.
struct PendingTextInsert: Identifiable, Equatable {
    let page: Int
    let point: PdfPoint
    var id: String { "\(page)-\(point.x)-\(point.y)" }
}

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
        if !on {
            contentTextEditor = nil
            textInsertArmed = false
            imageInsertBytes = nil
        }
    }

    /// Add Text: turns Edit Text on if it is off, then waits for a click.
    func armTextInsert(_ on: Bool) {
        guard on else {
            if textInsertArmed { store.editStatus = ContentEditing.insertCancelled }
            textInsertArmed = false
            return
        }
        if !store.contentEditMode { setContentEditMode(true) }
        guard store.contentEditMode else { return }
        imageInsertBytes = nil
        textInsertArmed = true
        store.editStatus = ContentEditing.insertPrompt
    }

    /// The sheet's Insert. Returns why it must stay open, or `nil` once the
    /// insertion is on its way.
    func insertText(_ text: String, size: String) -> String? {
        guard let pending = pendingTextInsert else { return nil }
        let (problem, request) = store.beginTextInsert(page: pending.page, at: pending.point, text: text, size: size)
        if let problem { return problem }
        pendingTextInsert = nil
        enqueuePageWork(request)
        return nil
    }

    func cancelTextInsert() {
        pendingTextInsert = nil
        store.editStatus = ContentEditing.insertCancelled
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
        if let bytes = imageInsertBytes {
            guard let point = annotationPoint(page: page, location: location, zoom: zoom) else { return true }
            imageInsertBytes = nil
            enqueuePageWork(store.beginImageInsert(page: page, at: point, bytes: bytes))
            return true
        }
        if textInsertArmed {
            // Placing a new line needs no content read: it goes where clicked.
            guard let point = annotationPoint(page: page, location: location, zoom: zoom) else { return true }
            textInsertArmed = false
            pendingTextInsert = PendingTextInsert(page: page, point: point)
            return true
        }
        guard store.pageContent[page] != nil else {
            readPageContent(page: page)
            store.editStatus = "Reading the page’s text… click again in a moment."
            return true
        }
        guard let point = annotationPoint(page: page, location: location, zoom: zoom),
              store.beginContentPress(page: page, at: point, zoom: zoom)
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

    /// The press ends: a click opens a line, a drag moves a line or an image,
    /// a corner drag resizes an image.
    func contentReleased(page: Int, location: CGPoint, zoom: Double) {
        guard store.contentDrag != nil || store.imageDrag != nil || store.pathDrag != nil else { return }
        guard let point = annotationPoint(page: page, location: location, zoom: zoom) else {
            return store.cancelContentDrag()
        }
        switch store.endContentDrag(at: point) {
        case let .open(block):
            if block.substitutesFont { store.editStatus = ContentEditing.fontSubstituted }
            contentTextEditor = block
        case let .move(request):
            enqueuePageWork(request)
        case .nothing:
            break
        }
    }

    /// Retypes a whole block — one run, or several as one undo step.
    func retypeText(_ block: ContentTextBlock, to text: String) {
        contentTextEditor = nil
        enqueuePageWork(store.beginContentEdit(ViewerStore.textRetype(block, to: text)))
    }

    func deleteText(_ block: ContentTextBlock) {
        contentTextEditor = nil
        enqueuePageWork(store.beginContentEdit(ViewerStore.textRemoval(block)))
    }

    /// Add Image…: a PNG or JPEG first, then the click that places it —
    /// Android's order. Turns Edit Content on if it is off.
    func armImageInsert() {
        guard canEditContent else { return }
        guard let url = prompts.chooseImage() else { return }
        guard let bytes = try? Data(contentsOf: url) else {
            store.editStatus = "The selected image could not be read."
            return
        }
        if !store.contentEditMode { setContentEditMode(true) }
        guard store.contentEditMode else { return }
        textInsertArmed = false
        imageInsertBytes = bytes
        store.editStatus = ContentEditing.imageInsertPrompt
    }

    /// Replace Image…: the core is asked first, so a refusal never costs the
    /// reader a file pick — Windows' order (T-204).
    func replaceSelectedImage() {
        if let refusal = store.selectedImageReplacementRefusal() {
            store.editStatus = refusal
            return
        }
        guard let url = prompts.chooseImage() else {
            store.editStatus = "Replace cancelled."
            return
        }
        guard let bytes = try? Data(contentsOf: url) else {
            store.editStatus = "The selected image could not be read."
            return
        }
        enqueuePageWork(store.beginSelectedImageReplace(with: bytes))
    }

    /// The selected image's Delete — the toolbar button and ⌫.
    func deleteSelectedImage() {
        enqueuePageWork(store.beginSelectedImageDelete())
    }

    /// The selected line or box's Delete Shape — the panel button and ⌫.
    func deleteSelectedPath() {
        enqueuePageWork(store.beginSelectedPathDelete())
    }

    func cancelTextEdit() {
        contentTextEditor = nil
        store.editStatus = "Edit cancelled."
    }
}
