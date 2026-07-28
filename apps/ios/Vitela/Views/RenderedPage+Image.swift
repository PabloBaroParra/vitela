import UIKit

extension RenderedPage {
    /// The UIKit half of the bridge; the pixel-buffer handling that produces
    /// the `CGImage` lives in apps/apple/Shared/RenderedPage+CGImage.swift.
    ///
    /// Scale is fixed at 1: `rgba` is already a device-pixel buffer rendered at
    /// the DPI the store asked for, so letting UIKit apply the screen scale
    /// again would draw the page at half size on every Retina device.
    var uiImage: UIImage? {
        guard let cgImage else { return nil }
        return UIImage(cgImage: cgImage, scale: 1, orientation: .up)
    }
}
