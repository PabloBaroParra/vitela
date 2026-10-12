// Page navigation: which page the viewport is on, and requests to scroll to
// another one. Mirrors Windows' `MainWindow.PageNavigation.cs` — the current
// page is the first page still visible at the top of the viewport, and every
// control (previous/next, first/last, go to page) moves relative to it.
//
// Scrolling itself is presentation: the store only publishes a
// `PageNavigationRequest`, and each shell's scroll view performs it.
import Foundation

/// One request to bring `page` to the top of the viewport. `serial` makes a
/// repeat request for the same page observable, so asking twice scrolls twice
/// (the user may have scrolled away in between).
struct PageNavigationRequest: Equatable {
    let page: Int
    let serial: UInt
}

/// A page's vertical extent relative to the top of the viewport, in view
/// points. Negative values lie above the visible area.
struct PageViewportSpan: Equatable {
    let top: Double
    let bottom: Double
}

/// The page the reader explicitly asked for — the twin of Windows'
/// `Viewer/RequestedPage.cs`. The viewport alone cannot say which page is
/// current after such a request: the last pages of a document cannot be
/// scrolled to the top at all, so the first visible page would be an earlier
/// one, and the readout would contradict the page that was picked. The
/// request is held while the page stays where the scroll left it, and
/// dropped as soon as the reader moves it.
struct RequestedPage: Equatable {
    /// Layout rounding between two reports of the same resting position.
    static let tolerance = 1.0

    let page: Int
    let serial: UInt
    /// Set once the scroll view has performed the request; spans reported
    /// before that are the scroll on its way, not the reader moving.
    var landed = false
    /// Where the page's top came to rest.
    var landedTop: Double?
}

extension ViewerStore {
    var pageCount: Int { document == nil ? 0 : pageSlots.count }

    var canGoToPreviousPage: Bool { pageCount > 0 && currentPage > 0 }

    var canGoToNextPage: Bool { pageCount > 0 && currentPage < pageCount - 1 }

    /// "Page 3 of 12", or `nil` with no document open.
    var pagePositionText: String? {
        guard pageCount > 0 else { return nil }
        return "Page \(currentPage + 1) of \(pageCount)"
    }

    /// Called by the scroll view with the span of every page row it currently
    /// has laid out. Rows a lazy stack keeps realized off screen are fine:
    /// one that ended above the viewport is skipped, and the first page still
    /// reaching into the viewport wins.
    func updateCurrentPage(spans: [Int: PageViewportSpan]) {
        if holdsRequestedPage(spans) { return }
        guard let first = spans
            .filter({ pageSlots.indices.contains($0.key) && $0.value.bottom > 0 })
            .map(\.key)
            .min()
        else { return }
        setCurrentPage(first)
    }

    func goToPage(_ index: Int) {
        guard pageCount > 0, (0..<pageCount).contains(index) else { return }
        navigationSerial &+= 1
        // Set now rather than waiting for the scroll to report back, so a
        // second step issued before the scroll settles moves from here.
        currentPage = index
        pageNavigation = PageNavigationRequest(page: index, serial: navigationSerial)
        requestedPage = RequestedPage(page: index, serial: navigationSerial)
    }

    /// The scroll view performed `request`. From here on, the requested page
    /// moving means the reader scrolled.
    func pageNavigationLanded(_ request: PageNavigationRequest) {
        guard requestedPage?.serial == request.serial else { return }
        requestedPage?.landed = true
    }

    /// Whether a pending request still decides the current page.
    private func holdsRequestedPage(_ spans: [Int: PageViewportSpan]) -> Bool {
        guard var request = requestedPage else { return false }
        if let span = spans[request.page] {
            if !request.landed || request.landedTop == nil {
                request.landedTop = span.top
                requestedPage = request
                setCurrentPage(request.page)
                return true
            }
            if let top = request.landedTop, abs(span.top - top) < RequestedPage.tolerance {
                setCurrentPage(request.page)
                return true
            }
        } else if !request.landed {
            return true
        }
        requestedPage = nil
        return false
    }

    private func setCurrentPage(_ page: Int) {
        if currentPage != page { currentPage = page }
    }

    func stepPage(by delta: Int) {
        guard pageCount > 0 else { return }
        goToPage(min(max(currentPage + delta, 0), pageCount - 1))
    }

    func goToFirstPage() {
        goToPage(0)
    }

    func goToLastPage() {
        goToPage(pageCount - 1)
    }

    /// The page index a typed 1-based page number names, or `nil` when the
    /// text is not a whole number within the document — the same rule
    /// Windows' Go to page dialog uses to enable its Go button.
    func pageIndex(forTypedNumber text: String) -> Int? {
        let trimmed = text.trimmingCharacters(in: .whitespaces)
        guard pageCount > 0, let number = Int(trimmed), (1...pageCount).contains(number) else { return nil }
        return number - 1
    }

    /// Called by `open` once a document has replaced the previous one.
    func resetNavigation() {
        currentPage = 0
        pageNavigation = nil
        requestedPage = nil
    }
}
