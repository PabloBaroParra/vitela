// Regenerates the macOS shell's AppIcon asset catalog from the brand mark.
//
//     swift scripts/macos-app-icon.swift
//
// AppKit renders assets/brand/vitela-app-mark.svg (NSImage reads SVG), so
// the Dock icon, Finder and the Mac App Store listing all come from the same
// source as Windows' .ico (scripts/windows-app-icon.py). As there, the navy
// mark sits on a white rounded plate that reads on light and dark Docks; the
// plate follows Apple's icon grid (824 of 1024 points, 100-point margin).
// Output: apps/macos/Vitela/Assets.xcassets/AppIcon.appiconset, 16–1024 px.
import AppKit

let root = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent()
let source = root.appendingPathComponent("assets/brand/vitela-app-mark.svg")
let catalog = root.appendingPathComponent("apps/macos/Vitela/Assets.xcassets")
let iconSet = catalog.appendingPathComponent("AppIcon.appiconset")

guard let mark = NSImage(contentsOf: source) else { fatalError("cannot read \(source.path)") }

let plateShare = 824.0 / 1024.0
let radiusShare = 0.2237
let markShare = 0.70

func render(pixels: Int) -> Data {
    let rep = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels,
                               bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                               colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    rep.size = NSSize(width: pixels, height: pixels)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
    let side = Double(pixels)
    let plateSide = side * plateShare
    let plate = NSRect(x: (side - plateSide) / 2, y: (side - plateSide) / 2, width: plateSide, height: plateSide)
    let shadow = NSShadow()
    shadow.shadowColor = NSColor.black.withAlphaComponent(0.25)
    shadow.shadowBlurRadius = side * 0.015
    shadow.shadowOffset = NSSize(width: 0, height: -side * 0.008)
    shadow.set()
    NSColor.white.setFill()
    NSBezierPath(roundedRect: plate, xRadius: plateSide * radiusShare, yRadius: plateSide * radiusShare).fill()
    NSShadow().set()
    let markSide = plateSide * markShare
    mark.draw(in: NSRect(x: (side - markSide) / 2, y: (side - markSide) / 2, width: markSide, height: markSide),
              from: .zero, operation: .sourceOver, fraction: 1)
    NSGraphicsContext.restoreGraphicsState()
    return rep.representation(using: .png, properties: [:])!
}

try FileManager.default.createDirectory(at: iconSet, withIntermediateDirectories: true)
var images: [[String: String]] = []
for points in [16, 32, 128, 256, 512] {
    for scale in [1, 2] {
        let name = "icon_\(points)x\(points)\(scale == 2 ? "@2x" : "").png"
        try render(pixels: points * scale).write(to: iconSet.appendingPathComponent(name))
        images.append(["filename": name, "idiom": "mac", "scale": "\(scale)x", "size": "\(points)x\(points)"])
    }
}
func json(_ object: Any) throws -> Data {
    try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
}
let info = ["author": "xcode", "version": 1] as [String: Any]
try json(["images": images, "info": info]).write(to: iconSet.appendingPathComponent("Contents.json"))
try json(["info": info]).write(to: catalog.appendingPathComponent("Contents.json"))
print("wrote \(images.count) icons to \(iconSet.path)")
