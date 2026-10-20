// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Add watermark through the generated UniFFI API — the calls Windows'
/// `AddWatermarkAsync` and Android's watermark tool make.
extension UniFfiPdfCoreClient {
    func defaultWatermarkLook() -> WatermarkLook {
        let look = Vitela.defaultWatermark(pages: [], text: "")
        return WatermarkLook(fontSize: look.fontSize, opacityPercent: look.opacity * 100, rotationDegrees: look.rotationDegrees)
    }

    /// The core's own look — its colour and layer — with the dialog's size,
    /// opacity and angle laid over it, as Windows records it.
    func addWatermark(handle: DocumentHandle, _ watermark: Watermark) throws {
        var stamp = Vitela.defaultWatermark(pages: watermark.pages.map { UInt32($0) }, text: watermark.text)
        stamp.fontSize = watermark.fontSize
        stamp.opacity = watermark.opacity
        stamp.rotationDegrees = watermark.rotationDegrees
        do {
            try Vitela.applyEdit(handle: handle, command: .addWatermark(watermark: stamp))
        } catch let FfiError.EncodingGap(character, _) {
            throw EditFailure.failed(Watermarking.encodingGap(character))
        } catch FfiError.UnsupportedOperation {
            throw EditFailure.notPermitted(handle.contentEditingAllowed() ? "This watermark cannot be added to these pages." : ContentEditing.forbidden)
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }
}
