// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here. The platform image wrapper (NSImage or
// UIImage) is built from this `CGImage` in each shell's Views directory; only
// the pixel-buffer handling below, which is the part easy to get wrong, is
// shared.
import CoreGraphics
import Foundation

extension RenderedPage {
    /// `pdf-render` hands back straight RGBA (`as_rgba_bytes`), so the byte
    /// order is `.premultipliedLast` over device RGB.
    ///
    /// `ViewerStore` has already checked that `rgba` holds at least
    /// `stride * height` bytes — Core Graphics does not bounds-check the
    /// provider, so that guarantee has to hold before we get here.
    var cgImage: CGImage? {
        guard let provider = CGDataProvider(data: rgba as CFData) else { return nil }
        return CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: stride,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: true,
            intent: .defaultIntent
        )
    }
}
