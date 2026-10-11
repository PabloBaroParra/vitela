import AppKit

/// File ▸ Print…: the standard print panel over `PrintPagesView`.
///
/// `NSPrintOperation.run()` is app-modal and draws on the main thread, so
/// each page's render blocks the run loop while it rasterizes — acceptable
/// for an operation the user has explicitly started, and the same trade the
/// Linux shell makes. The progress panel keeps the wait visible.
extension ViewerViewModel {
    func printDocument() {
        makePrintOperation()?.run()
    }

    /// `nil` when there is nothing to print (the store's status line says
    /// why). Split from `printDocument` so tests can drive the operation
    /// without a panel.
    func makePrintOperation() -> NSPrintOperation? {
        guard let request = store.beginPrint() else { return nil }
        let printInfo = Self.printInfo()
        let store = store
        let view = PrintPagesView(pages: request.pages, printInfo: printInfo) { page, dpi in
            store.printPage(page, for: request, dpi: dpi)
        }
        let operation = NSPrintOperation(view: view, printInfo: printInfo)
        operation.jobTitle = documentName ?? "Vitela"
        operation.showsPrintPanel = true
        operation.showsProgressPanel = true
        operation.printPanel.options.formUnion([.showsPaperSize, .showsOrientation, .showsScaling, .showsPreview])
        return operation
    }

    /// The shared print info, with the margins pulled in to the printer's
    /// own printable area so a page uses as much of the paper as it can.
    private static func printInfo() -> NSPrintInfo {
        // A copy: the shared instance is the app-wide default, and a job's
        // margin tweaks must not leak into the next one.
        let info = (NSPrintInfo.shared.copy() as? NSPrintInfo) ?? NSPrintInfo()
        let printable = info.imageablePageBounds
        if !printable.isEmpty {
            info.leftMargin = printable.minX
            info.bottomMargin = printable.minY
            info.rightMargin = max(info.paperSize.width - printable.maxX, 0)
            info.topMargin = max(info.paperSize.height - printable.maxY, 0)
        }
        info.isHorizontallyCentered = true
        info.isVerticallyCentered = true
        return info
    }
}
