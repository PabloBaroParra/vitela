// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import CoreGraphics
import Foundation

/// One print job's view of the document, taken when the user asks to print.
/// Like `RenderRequest`, it carries its own document, so a page rendered for
/// the printer never reads back into the store — and a document opened while
/// the dialog is up does not change what this job prints.
struct PrintRequest {
    let generation: UInt
    let document: any PdfDocument
    let pages: [PageDimensions]
}

/// Printing, mirroring the Linux (`app/print.rs`) and Windows
/// (`MainWindow.Print.cs`) shells: each page is rasterized once, on demand,
/// at a fixed print-quality DPI — independent of the viewer's zoom and of its
/// render queue, so a page scrolling out of view can never supersede a page
/// the printer still needs.
///
/// Prints the open handle itself. That is complete today because the Apple
/// shells only edit the Info dictionary, which never reaches the page. Once
/// they can annotate, this must print an output snapshot instead, as Linux
/// and Windows do, or the printout will miss what the overlay draws.
extension ViewerStore {
    /// Full-quality rasterization for the printer; the platform scales the
    /// bitmap onto the paper.
    static let printDpi = 300
    /// The print dialog's preview is a thumbnail of the paper, so a
    /// screen-grade DPI is plenty — the same value Windows previews at.
    static let printPreviewDpi = 144

    /// `nil` when there is nothing to print; the status line says why.
    func beginPrint() -> PrintRequest? {
        guard let document else {
            editStatus = "Open a PDF before printing."
            return nil
        }
        guard !document.pages.isEmpty else {
            editStatus = "The PDF has no pages to print."
            return nil
        }
        return PrintRequest(generation: generation, document: document, pages: document.pages)
    }

    /// Thread-safe, like `renderResult(for:)`: touches only the immutable
    /// client and the request's own document. `nil` when the page cannot be
    /// rendered or the core returned an unusable buffer — the job prints that
    /// page blank rather than aborting, which is what Linux does too.
    func printPage(_ page: Int, for request: PrintRequest, dpi: Int) -> RenderedPage? {
        guard request.pages.indices.contains(page),
              let image = try? client.render(document: request.document, page: page, dpi: dpi),
              isDisplayable(image)
        else { return nil }
        return image
    }
}

enum PrintLayout {
    /// The largest rect with `page`'s aspect ratio that fits `bounds`,
    /// centred in it — a page is scaled to the paper, never cropped or
    /// stretched.
    static func fit(_ page: PageDimensions, in bounds: CGRect) -> CGRect {
        guard page.width > 0, page.height > 0, bounds.width > 0, bounds.height > 0 else { return .zero }
        let scale = min(bounds.width / CGFloat(page.width), bounds.height / CGFloat(page.height))
        let size = CGSize(width: CGFloat(page.width) * scale, height: CGFloat(page.height) * scale)
        return CGRect(
            x: bounds.midX - size.width / 2,
            y: bounds.midY - size.height / 2,
            width: size.width,
            height: size.height
        )
    }
}
