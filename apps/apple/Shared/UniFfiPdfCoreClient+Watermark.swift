// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Add watermark through the generated UniFFI API — the calls Windows'
/// `AddWatermarkAsync` and Android's watermark tool make, plus the picture
/// half Linux records.
extension UniFfiPdfCoreClient {
    func defaultWatermarkLook(_ kind: WatermarkKind) -> WatermarkLook {
        switch kind {
        case .text:
            let look = Vitela.defaultWatermark(pages: [], text: "")
            return WatermarkLook(
                size: look.fontSize, opacityPercent: look.opacity * 100, rotationDegrees: look.rotationDegrees, layer: WatermarkLayer(look.layer)
            )
        case .picture:
            let look = Vitela.defaultImageWatermark(pages: [], image: Data())
            return WatermarkLook(
                size: look.scale * 100, opacityPercent: look.opacity * 100, rotationDegrees: look.rotationDegrees, layer: WatermarkLayer(look.layer)
            )
        }
    }

    /// The core's own look — a text stamp's colour — with the dialog's
    /// answers laid over it, as Windows records it.
    func addWatermark(handle: DocumentHandle, _ watermark: Watermark) throws {
        let pages = watermark.pages.map { UInt32($0) }
        let command: FfiEditCommand
        switch watermark.mark {
        case let .text(text, fontSize):
            var stamp = Vitela.defaultWatermark(pages: pages, text: text)
            stamp.fontSize = fontSize
            stamp.opacity = watermark.opacity
            stamp.rotationDegrees = watermark.rotationDegrees
            stamp.layer = FfiWatermarkLayer(watermark.layer)
            command = .addWatermark(watermark: stamp)
        case let .picture(bytes, scale):
            var stamp = Vitela.defaultImageWatermark(pages: pages, image: bytes)
            stamp.scale = scale
            stamp.opacity = watermark.opacity
            stamp.rotationDegrees = watermark.rotationDegrees
            stamp.layer = FfiWatermarkLayer(watermark.layer)
            command = .addImageWatermark(watermark: stamp)
        }
        do {
            try Vitela.applyEdit(handle: handle, command: command)
        } catch let FfiError.EncodingGap(character, _) {
            throw EditFailure.failed(Watermarking.encodingGap(character))
        } catch FfiError.InvalidImage {
            throw EditFailure.failed(Watermarking.notAPicture)
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(handle.contentEditingAllowed() ? "This watermark cannot be added to these pages." : ContentEditing.forbidden)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}

private extension WatermarkLayer {
    init(_ layer: FfiWatermarkLayer) {
        switch layer {
        case .overContent: self = .overContent
        case .underContent: self = .underContent
        }
    }
}

private extension FfiWatermarkLayer {
    init(_ layer: WatermarkLayer) {
        switch layer {
        case .overContent: self = .overContent
        case .underContent: self = .underContent
        }
    }
}
