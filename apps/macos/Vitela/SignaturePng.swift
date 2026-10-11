import AppKit
import CoreGraphics

/// Turns pad strokes into the PNG the image stamp takes: black ink with round
/// caps on a transparent background, cropped by `DrawnSignature.frame`. The
/// stroke → PNG step is the shell's on every platform (Windows'
/// `SignaturePng`, Android's), because the core's stamp builder takes encoded
/// image bytes; here Core Graphics draws the anti-aliased line.
enum SignaturePng {
    /// The PNG, or `nil` when the pad holds no line.
    static func render(_ strokes: [[PadPoint]], strokeWidth: Double = DrawnSignature.strokeWidth) -> Data? {
        guard let frame = DrawnSignature.frame(strokes, strokeWidth: strokeWidth),
              let context = CGContext(
                  data: nil, width: frame.pixelWidth, height: frame.pixelHeight, bitsPerComponent: 8, bytesPerRow: 0,
                  space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
              )
        else { return nil }
        // Pad space has a top-left origin; flip so the image is upright.
        context.translateBy(x: 0, y: CGFloat(frame.pixelHeight))
        context.scaleBy(x: CGFloat(frame.scale), y: -CGFloat(frame.scale))
        context.translateBy(x: -CGFloat(frame.left), y: -CGFloat(frame.top))
        context.setStrokeColor(CGColor(gray: 0, alpha: 1))
        context.setLineWidth(CGFloat(strokeWidth))
        context.setLineCap(.round)
        context.setLineJoin(.round)
        for stroke in strokes where stroke.count >= 2 {
            context.beginPath()
            context.move(to: CGPoint(x: stroke[0].x, y: stroke[0].y))
            for point in stroke.dropFirst() { context.addLine(to: CGPoint(x: point.x, y: point.y)) }
            context.strokePath()
        }
        guard let image = context.makeImage() else { return nil }
        return NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:])
    }
}
