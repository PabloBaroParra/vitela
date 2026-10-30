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
    /// Only a replace still waits for a save: the core reads the original
    /// picture back for its undo, and an unsaved edit has none in the file.
    static let imageAgain = "Save before replacing this image: it has an unsaved move, resize or deletion."
    /// Linux's `IMAGE_SELECTED_NO_REPLACE` and Windows' hint, said when the
    /// image is selected rather than after a file was picked for it.
    static let imageSelectedNoReplace = "Image selected — move, resize or delete it. It cannot be replaced: "
        + "its current encoding cannot be read back, so the swap could not be undone."

    var selectedContentImage: ContentImage? { contentEdit.selectedImage }

    var imageDrag: ImageDrag? { contentEdit.imageDrag }

    /// Grabs the selected image by a corner or its body, or (unless
    /// `onlySelected`) the topmost image under `point`, selecting it. An
    /// image edited before a save can be grabbed again: the core folds the
    /// new box into the queued edit (B25 T-207, Windows' `ImageGestures`).
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
        contentEdit.selectedPath = nil
        contentEdit.selectedImage = hit
        let key = ContentImageKey(page: hit.page, id: hit.id)
        if !contentEdit.replaceChecked.contains(key), let document {
            contentEdit.replaceChecked.insert(key)
            if client.imageReplacementRefusal(document: document, image: hit) != nil {
                contentEdit.unreplaceableImages.insert(key)
            }
        }
        contentEdit.imageDrag = ImageDrag(image: hit, origin: point, current: point, corner: hit == selected ? corner : nil)
        editStatus = contentEdit.unreplaceableImages.contains(key)
            ? Self.imageSelectedNoReplace
            : "Image selected. Drag it to move it, a corner to resize it, or delete it."
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
        guard let request = beginContentEdit(edit) else { return .nothing }
        contentEdit.reselectImage = ContentImageKey(page: drag.image.page, id: drag.image.id)
        return .move(request)
    }

    /// The selected image's Delete.
    func beginSelectedImageDelete() -> PageWorkRequest? {
        guard contentEditMode, let image = contentEdit.selectedImage else { return nil }
        contentEdit.reselectImage = nil
        return beginContentEdit(.removeImage(image))
    }

    /// Whether Replace Image… is offered for the selected image: not one held
    /// since an edit, nor one the core already said it cannot read back —
    /// Windows' `ImageControls.Ready`. `selectedImageReplacementRefusal`
    /// still asks the core again when the button is pressed.
    var canReplaceSelectedImage: Bool {
        guard contentEditMode, let image = contentEdit.selectedImage else { return false }
        let key = ContentImageKey(page: image.page, id: image.id)
        return !contentEdit.editedImages.contains(key) && !contentEdit.unreplaceableImages.contains(key)
    }

    /// The image an edit just landed on, selected again at its new box once
    /// its page is read back, so its frame follows it — Windows'.
    func reselectImage(on page: Int, in content: PageContent) {
        guard contentEditMode, let key = contentEdit.reselectImage, key.page == page else { return }
        contentEdit.reselectImage = nil
        contentEdit.selectedImage = content.images.first { $0.id == key.id }
    }

    /// Replace Image… may go ahead for the selected image: not one held since
    /// an edit, and not one the core will not read back for undo. Main
    /// thread, like the other permission questions.
    func selectedImageReplacementRefusal() -> String? {
        guard contentEditMode, let document, let image = contentEdit.selectedImage else { return "Select an image first." }
        if contentEdit.editedImages.contains(ContentImageKey(page: image.page, id: image.id)) { return Self.imageAgain }
        return client.imageReplacementRefusal(document: document, image: image)
    }

    /// The selected image's picture, replaced by `bytes`.
    func beginSelectedImageReplace(with bytes: Data) -> PageWorkRequest? {
        guard contentEditMode, let image = contentEdit.selectedImage else { return nil }
        return beginContentEdit(.replaceImage(image, with: bytes))
    }

    /// A new image with its top-left corner at `point` on `page`, sized by the
    /// core's stamp policy so every shell agrees — Android's insertion.
    func beginImageInsert(page: Int, at point: PdfPoint, bytes: Data) -> PageWorkRequest? {
        guard contentEditMode else { return nil }
        let rect: PdfRect
        do {
            rect = try client.stampPlacement(imageBytes: bytes, anchor: point)
        } catch {
            editStatus = Self.editFailure(error).localizedDescription
            return nil
        }
        return beginContentEdit(.insertImage(page: page, rect: rect, bytes: bytes))
    }

    /// Called once page work landed: an image edit holds that image's
    /// replace until a save; an undo or redo gives every image back.
    func noteContentWork(_ work: PageWork) {
        switch work {
        case let .editContent(.moveImage(image, _)), let .editContent(.resizeImage(image, _)),
             let .editContent(.removeImage(image)), let .editContent(.replaceImage(image, _)):
            contentEdit.editedImages.insert(ContentImageKey(page: image.page, id: image.id))
        case let .editContent(.moveSelection(selection, _, _)), let .editContent(.removeSelection(selection)):
            // Their replace waits for a save, like a lone image edit's.
            for image in selection.images { contentEdit.editedImages.insert(ContentImageKey(page: image.page, id: image.id)) }
        case .refresh:
            contentEdit.editedImages = []
            contentEdit.replaceChecked = []
            contentEdit.unreplaceableImages = []
        default:
            break
        }
    }
}
