import Foundation

/// Renders only for pages the reader can still see — Windows' `PageWindow`
/// ("is this page still worth rendering?") and Linux cancelling renders
/// that scrolled out of its prefetch range. A fast scroll through a long
/// document queues a render for every row that flashed past; without this
/// the serial queue works through all of them before the page now in view.
extension ViewerViewModel {
    /// A page row left the view: its render, if still waiting in the queue,
    /// is dropped. One already running finishes and lands as usual.
    func pageLeftView(_ page: Int, generation: UInt) {
        guard let queued = renderOperations[page], queued.generation == generation,
              !queued.operation.isExecuting, !queued.operation.isFinished else { return }
        queued.operation.cancel()
    }

    /// Whether `page`'s last render was dropped before it ran. Its slot still
    /// reads as rendering, so asking again has to insist.
    func renderWasDropped(_ page: Int) -> Bool {
        guard let queued = renderOperations[page] else { return false }
        return queued.generation == store.generation && queued.operation.isCancelled
    }
}
