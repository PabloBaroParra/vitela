import AppKit

/// An image from outside the app placed on a page as a stamp: pasted from
/// the clipboard (⌘V) or dropped on a page — Linux's T-050/T-051
/// (`input.rs`), Windows' T-063 (`MainWindow.FileDrop.cs`). The stamp itself
/// is the store's (`ViewerStore+Stamps.swift`).
extension ViewerViewModel {
    /// Only bitmap types are read — never a string, so a copied URL cannot
    /// turn into a download, Linux's rule.
    static let pasteboardImageTypes: [NSPasteboard.PasteboardType] = [.png, .tiff]

    /// ⌘V outside a text field: the clipboard's bitmap, re-encoded as PNG,
    /// hung from the centre of the page in view.
    func pasteImageStamp(from pasteboard: NSPasteboard = .general) {
        guard store.document != nil else {
            store.editStatus = ViewerStore.pasteNeedsDocument
            return
        }
        guard let png = Self.pngFromPasteboard(pasteboard) else {
            store.editStatus = ViewerStore.clipboardHasNoImage
            return
        }
        store.pasteStamp(imageBytes: png)
    }

    /// A PNG as it is, any other bitmap re-encoded as one: the clipboard
    /// hands over a TIFF for most copies, and the core takes PNG or JPEG.
    static func pngFromPasteboard(_ pasteboard: NSPasteboard) -> Data? {
        if let png = pasteboard.data(forType: .png), ViewerStore.isSupportedStampImage(png) { return png }
        guard let tiff = pasteboard.data(forType: .tiff), let bitmap = NSBitmapImageRep(data: tiff) else { return nil }
        return bitmap.representation(using: .png, properties: [:])
    }

    /// Whether `event` is the paste this monitor takes: ⌘V, with no text
    /// field editing (a field's own paste comes first, Windows'
    /// `TextInputHasFocus`) and not over Organize, which has no page in view.
    func takesPaste(_ event: NSEvent) -> Bool {
        event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command
            && event.charactersIgnoringModifiers == "v"
            && !(event.window?.firstResponder is NSText)
            && !isOrganizing
    }

    /// Files dropped on `page` at `location`: an image becomes a stamp hung
    /// from the drop point; a PDF opens as the window-level drop would.
    func dropFiles(_ urls: [URL], onPage page: Int, at location: CGPoint, zoom: Double) {
        guard let url = DroppedFileKind.firstFile(urls) else {
            notice = Self.unsupportedDropMessage
            return
        }
        operationQueue.addOperation { [weak self] in
            let bytes = try? Data(contentsOf: url)
            DispatchQueue.main.async {
                guard let self else { return }
                guard let bytes, DroppedFileKind.classify(bytes) == .image else {
                    return self.openExternalFiles([url])
                }
                guard let point = self.annotationPoint(page: page, location: location, zoom: zoom) else {
                    self.store.editStatus = "The page is not ready for image placement."
                    return
                }
                self.store.insertStamp(imageBytes: bytes, page: page, rect: nil, anchor: point)
            }
        }
    }
}
