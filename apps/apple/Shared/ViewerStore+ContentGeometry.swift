// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Typing where page content goes and how big an image is — Windows'
/// `MainWindow.Images.cs` (Move / Resize image) and
/// `MainWindow.TextGeometry.cs` (Move text), over the same content edits a
/// drag records. Each answer is a sheet's: a problem keeps it open, a
/// request is the edit on its way, and neither means nothing changed.
extension ViewerStore {
    enum ContentGeometry {
        static let imageHint = "Coordinates use PDF space: X increases rightward and Y upward, from the image's bottom-left corner."
        static let textHint = "Coordinates use PDF space: X increases rightward and Y upward. The text keeps its font and size. Save and reopen first if existing text has been retyped."
        static let imagePositionUnchanged = "Image position unchanged."
        static let imageSizeUnchanged = "Image dimensions unchanged."
        static let invalidPosition = "Enter a finite X and Y in points."
        static let invalidSize = "Image dimensions must be finite and greater than zero."
    }

    /// The selected image with its bottom-left corner at `x`, `y`, its size kept.
    func beginSelectedImageMove(toX x: Double, y: Double) -> (problem: String?, request: PageWorkRequest?) {
        let (image, refusal) = editableSelectedImage()
        guard let image else { return (refusal, nil) }
        guard x.isFinite, y.isFinite else { return (ContentGeometry.invalidPosition, nil) }
        let to = PdfRect(x: x, y: y, width: image.bbox.width, height: image.bbox.height)
        guard to != image.bbox else {
            editStatus = ContentGeometry.imagePositionUnchanged
            return (nil, nil)
        }
        return (nil, beginImageGeometry(.moveImage(image, to: to), image))
    }

    /// The selected image `width` × `height` points from its current origin.
    func beginSelectedImageResize(width: Double, height: Double) -> (problem: String?, request: PageWorkRequest?) {
        let (image, refusal) = editableSelectedImage()
        guard let image else { return (refusal, nil) }
        guard width.isFinite, height.isFinite, width > 0, height > 0 else { return (ContentGeometry.invalidSize, nil) }
        let to = PdfRect(x: image.bbox.x, y: image.bbox.y, width: width, height: height)
        guard to != image.bbox else {
            editStatus = ContentGeometry.imageSizeUnchanged
            return (nil, nil)
        }
        return (nil, beginImageGeometry(.resizeImage(image, to: to), image))
    }

    /// `block` with its box's bottom-left corner at `x`, `y` — the same move
    /// a drag records, so a line moved again before a save is one more move.
    func beginTextMove(_ block: ContentTextBlock, toX x: Double, y: Double) -> (problem: String?, request: PageWorkRequest?) {
        guard x.isFinite, y.isFinite else { return (ContentGeometry.invalidPosition, nil) }
        let to = PdfRect(x: x, y: y, width: block.bbox.width, height: block.bbox.height)
        guard to != block.bbox else {
            editStatus = ContentEditing.positionUnchanged
            return (nil, nil)
        }
        return (nil, beginContentEdit(Self.textMove(block, to: to)))
    }

    /// The selected image, or why there is none to edit. One edited before
    /// a save can be edited again: the core folds the new box in.
    private func editableSelectedImage() -> (image: ContentImage?, refusal: String?) {
        guard contentEditMode, let image = contentEdit.selectedImage else { return (nil, "Select an image first.") }
        return (image, nil)
    }

    private func beginImageGeometry(_ edit: ContentEdit, _ image: ContentImage) -> PageWorkRequest? {
        guard let request = beginContentEdit(edit) else { return nil }
        contentEdit.reselectImage = ContentImageKey(page: image.page, id: image.id)
        return request
    }
}
