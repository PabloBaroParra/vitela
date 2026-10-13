// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// A press on an image in Edit Text: its body moves it, a corner of the
/// selected image resizes it — Windows' `ContentImageDrag`, with the same
/// corner maths as an annotation's (`ViewerStore.resizedRect`).
struct ImageDrag: Equatable {
    let image: ContentImage
    let origin: PdfPoint
    var current: PdfPoint
    let corner: AnnotationCorner?

    /// Where the image would be if released here.
    var previewRect: PdfRect {
        if let corner { return ViewerStore.resizedRect(image.bbox, corner: corner, to: current) }
        return PdfRect(x: image.bbox.x + current.x - origin.x, y: image.bbox.y + current.y - origin.y,
                       width: image.bbox.width, height: image.bbox.height)
    }
}

/// Selecting, moving, resizing and deleting an image the page paints —
/// Windows' `MainWindow.ImageGestures.cs` / `PdfDocumentFacade.Images.cs`,
/// Linux's `content_edit/image.rs`.
extension ViewerStore {
    static let imageAgain = "Save and reopen before editing this image again."

    var selectedContentImage: ContentImage? { contentEdit.selectedImage }

    var imageDrag: ImageDrag? { contentEdit.imageDrag }

    /// Grabs the selected image by a corner or its body, or (unless
    /// `onlySelected`) the topmost image under `point`, selecting it. An image
    /// edited since the document was opened is selected but not grabbed.
    func beginImageDrag(page: Int, at point: PdfPoint, zoom: Double, onlySelected: Bool) -> Bool {
        let selected = contentEdit.selectedImage.flatMap { $0.page == page ? $0 : nil }
        let reach = Self.handleReach / max(zoom, 0.01)
        let corner = selected.flatMap { Self.corner(of: $0.bbox, at: point, reach: reach) }
        let hit: ContentImage?
        if let selected, corner != nil || selected.bbox.contains(point) {
            hit = selected
        } else if !onlySelected {
            hit = pageContent[page]?.images.last { $0.bbox.contains(point) }
        } else {
            hit = nil
        }
        guard let hit else { return false }
        contentEdit.selectedImage = hit
        guard !contentEdit.editedImages.contains(ContentImageKey(page: hit.page, id: hit.id)) else {
            editStatus = Self.imageAgain
            return true
        }
        contentEdit.imageDrag = ImageDrag(image: hit, origin: point, current: point, corner: hit == selected ? corner : nil)
        editStatus = "Image selected. Drag it to move it, a corner to resize it, or delete it."
        return true
    }

    func continueImageDrag(to point: PdfPoint) {
        contentEdit.imageDrag?.current = point
    }

    /// A release that left the image where it was only selects it.
    func endImageDrag(at point: PdfPoint) -> ContentRelease {
        continueImageDrag(to: point)
        guard let drag = contentEdit.imageDrag else { return .nothing }
        contentEdit.imageDrag = nil
        let to = drag.previewRect
        guard to != drag.image.bbox else { return .nothing }
        guard to.width > 0, to.height > 0 else {
            editStatus = "Image dimensions must be finite and greater than zero."
            return .nothing
        }
        let edit: ContentEdit = drag.corner == nil ? .moveImage(drag.image, to: to) : .resizeImage(drag.image, to: to)
        return beginContentEdit(edit).map(ContentRelease.move) ?? .nothing
    }

    /// The selected image's Delete.
    func beginSelectedImageDelete() -> PageWorkRequest? {
        guard contentEditMode, let image = contentEdit.selectedImage else { return nil }
        guard !contentEdit.editedImages.contains(ContentImageKey(page: image.page, id: image.id)) else {
            editStatus = Self.imageAgain
            return nil
        }
        return beginContentEdit(.removeImage(image))
    }

    /// Called once page work landed: an image edit holds that image until
    /// the document is reopened; an undo or redo gives every image back.
    func noteContentWork(_ work: PageWork) {
        switch work {
        case let .editContent(.moveImage(image, _)), let .editContent(.resizeImage(image, _)), let .editContent(.removeImage(image)):
            contentEdit.editedImages.insert(ContentImageKey(page: image.page, id: image.id))
        case .refresh:
            contentEdit.editedImages = []
        default:
            break
        }
    }
}
