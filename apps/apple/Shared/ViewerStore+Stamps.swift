// Image stamps: a PNG or JPEG placed on a page. Mirrors Windows'
// `MainWindow.StampPlacement.cs`, `InsertStampFromImageBytesAsync` and
// `Viewer/ImageStampInput.cs`: a drag with the Stamp tool marks where it
// goes, the shell asks for the image, the core inserts it as one undoable
// edit, and the new stamp lands selected. The shell keeps the image bytes
// only to draw a preview until a save bakes the stamp into the page.
import Foundation

/// A stamp traced on a page, waiting for its image.
struct PendingStamp: Equatable {
    let page: Int
    /// The dragged rect, or `nil` for a click — the stamp then takes the
    /// image's own proportions from the core, anchored at `anchor`.
    let rect: PdfRect?
    let anchor: PdfPoint
}

extension ViewerStore {
    private static let pngSignature = Data([137, 80, 78, 71, 13, 10, 26, 10])

    /// Checks only the encoding signature; the core's decode is the
    /// authoritative one, so a corrupt-but-recognizable image gets its error.
    static func isSupportedStampImage(_ bytes: Data) -> Bool {
        bytes.starts(with: pngSignature) || bytes.starts(with: [0xFF, 0xD8, 0xFF])
    }

    /// Where a Stamp drag ended: a click or a drag shorter than the slop on
    /// both axes lets the image decide its size.
    func beginStampPlacement(page: Int, origin: PdfPoint, release: PdfPoint) {
        let tap = abs(release.x - origin.x) < Self.freeTextTapSlop && abs(release.y - origin.y) < Self.freeTextTapSlop
        let rect = tap ? nil : Self.tracedRect(from: origin, to: release, tool: .stamp)
        pendingStamp = PendingStamp(page: page, rect: rect, anchor: origin)
    }

    func cancelPendingStamp() {
        guard pendingStamp != nil else { return }
        pendingStamp = nil
        editStatus = "Stamp placement canceled."
    }

    /// Inserts `imageBytes` where the pending stamp was traced.
    func insertPendingStamp(imageBytes: Data) {
        guard let pending = pendingStamp else { return }
        pendingStamp = nil
        insertStamp(imageBytes: imageBytes, page: pending.page, rect: pending.rect, anchor: pending.anchor)
    }

    func insertStamp(imageBytes: Data, page: Int, rect: PdfRect?, anchor: PdfPoint) {
        guard let document, annotationEditingAllowed, pageSlots.indices.contains(page) else { return }
        guard Self.isSupportedStampImage(imageBytes) else {
            editStatus = "The image is not a supported PNG or JPEG."
            return
        }
        let before = Set(annotations.map(\.id))
        do {
            let placed = try rect ?? client.stampPlacement(imageBytes: imageBytes, anchor: anchor)
            try client.insertImageStamp(document: document, page: page, imageBytes: imageBytes, rect: placed)
        } catch {
            editStatus = Self.editMessage(for: error)
            return
        }
        refreshAnnotations()
        // Arrives where the reader pointed, at a size they will often want to
        // change, so it lands selected — as on both other shells.
        if let added = annotations.last(where: { !before.contains($0.id) }) {
            selectedAnnotationId = added.id
            stampImages[added.id] = imageBytes
        }
        recordEdit(status: "Image stamp added. Changes are pending save.")
    }

    /// Pasting with no editable document: Linux's and Windows' wording.
    static let pasteNeedsDocument = "Open an editable PDF before pasting an image."
    static let clipboardHasNoImage = "Clipboard does not contain a bitmap image."

    /// A pasted image, hung from the centre of the page in view — Linux's
    /// `paste_target`, Windows' `PasteImage_Invoked`. The target is read
    /// here, when the bytes are already in hand, so nothing can retarget it.
    func pasteStamp(imageBytes: Data) {
        guard document != nil, annotationEditingAllowed, !pageSlots.isEmpty else {
            editStatus = Self.pasteNeedsDocument
            return
        }
        let page = min(max(currentPage, 0), pageSlots.count - 1)
        guard let placement = placement(ofPage: page, zoom: 1) else { return }
        let dimensions = pageSlots[page].dimensions
        let centre = client.pointToPdf(x: dimensions.width / 2, y: dimensions.height / 2, on: placement)
        insertStamp(imageBytes: imageBytes, page: page, rect: nil, anchor: centre)
    }
}
