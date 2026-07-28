import AppKit

extension RenderedPage {
    /// The AppKit half of the bridge; the pixel-buffer handling that produces
    /// the `CGImage` lives in apps/apple/Shared/RenderedPage+CGImage.swift.
    var nsImage: NSImage? {
        guard let cgImage else { return nil }
        return NSImage(cgImage: cgImage, size: NSSize(width: width, height: height))
    }
}
