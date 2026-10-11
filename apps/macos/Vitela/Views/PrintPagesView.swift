import AppKit

/// The view an `NSPrintOperation` paginates: one printer page per PDF page,
/// stacked top to bottom, each the size of the paper's printable area.
///
/// Nothing is rendered up front. `draw(_:)` asks for one page's bitmap when
/// AppKit draws that page — at `ViewerStore.printPreviewDpi` for the print
/// panel's preview, `ViewerStore.printDpi` for the printer — and drops it
/// again, so a long document never holds every page's pixels at once.
final class PrintPagesView: NSView {
    private let pages: [PageDimensions]
    private let renderPage: (_ page: Int, _ dpi: Int) -> RenderedPage?
    private let fallbackPrintInfo: NSPrintInfo

    /// `renderPage` is called on whatever thread AppKit draws on.
    init(pages: [PageDimensions], printInfo: NSPrintInfo, renderPage: @escaping (_ page: Int, _ dpi: Int) -> RenderedPage?) {
        self.pages = pages
        self.renderPage = renderPage
        fallbackPrintInfo = printInfo
        super.init(frame: .zero)
        setFrameSize(NSSize(width: pageSize.width, height: pageSize.height * CGFloat(pages.count)))
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) { nil }

    override var isFlipped: Bool { true }

    /// The paper's printable area. Read from the running operation, not
    /// cached: picking another paper size or orientation in the panel
    /// re-paginates through `knowsPageRange`, and the pages must follow.
    var pageSize: NSSize {
        let info = NSPrintOperation.current?.printInfo ?? fallbackPrintInfo
        return NSSize(
            width: max(info.paperSize.width - info.leftMargin - info.rightMargin, 1),
            height: max(info.paperSize.height - info.topMargin - info.bottomMargin, 1)
        )
    }

    override func knowsPageRange(_ range: NSRangePointer) -> Bool {
        let size = pageSize
        setFrameSize(NSSize(width: size.width, height: size.height * CGFloat(pages.count)))
        range.pointee = NSRange(location: 1, length: pages.count)
        return true
    }

    /// `page` is 1-based, as AppKit numbers them.
    override func rectForPage(_ page: Int) -> NSRect {
        let size = pageSize
        return NSRect(x: 0, y: CGFloat(page - 1) * size.height, width: size.width, height: size.height)
    }

    /// When printing, AppKit draws one page's rect at a time, so this renders
    /// exactly one page per call. Anything else asking for a larger rect gets
    /// every page it overlaps.
    override func draw(_ dirtyRect: NSRect) {
        let dpi = NSGraphicsContext.currentContextDrawingToScreen() ? ViewerStore.printPreviewDpi : ViewerStore.printDpi
        for index in pages.indices {
            let pageRect = rectForPage(index + 1)
            let overlap = pageRect.intersection(dirtyRect)
            guard overlap.width > 0, overlap.height > 0 else { continue }
            // A page that will not render is left blank, not fatal to the job.
            guard let image = renderPage(index, dpi)?.nsImage else { continue }
            // `respectFlipped` keeps the bitmap upright in this flipped view.
            image.draw(
                in: PrintLayout.fit(pages[index], in: pageRect), from: .zero, operation: .sourceOver, fraction: 1,
                respectFlipped: true, hints: [.interpolation: NSImageInterpolation.high.rawValue]
            )
        }
    }
}
