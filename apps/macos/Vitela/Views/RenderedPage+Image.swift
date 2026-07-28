import AppKit
import CoreGraphics

extension RenderedPage {
    /// `pdf-render` hands back straight RGBA (`as_rgba_bytes`), so the byte
    /// order is `.premultipliedLast` over device RGB.
    ///
    /// `ViewerStore` has already checked that `rgba` holds at least
    /// `stride * height` bytes — Core Graphics does not bounds-check the
    /// provider, so that guarantee has to hold before we get here.
    var nsImage: NSImage? {
        guard let provider = CGDataProvider(data: rgba as CFData),
              let image = CGImage(
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
              ) else { return nil }
        return NSImage(cgImage: image, size: NSSize(width: width, height: height))
    }
}
