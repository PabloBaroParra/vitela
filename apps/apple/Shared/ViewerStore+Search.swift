// Document-wide text search: issuing a query in the background, guarding
// against a superseded result, and stepping through matches. Mirrors the
// GTK shell's `app/search.rs`.
import Foundation

extension ViewerStore {
    /// A self-contained unit of search work, mirroring `RenderRequest`.
    struct SearchRequest {
        let generation: UInt
        let document: any PdfDocument
        let query: String
    }

    /// `nil` when there is nothing to search — no document open, or an
    /// empty query (the caller should `clearSearch()` in that case instead).
    func beginSearch(query: String) -> SearchRequest? {
        guard !query.isEmpty, let document else { return nil }
        return SearchRequest(generation: generation, document: document, query: query)
    }

    /// Thread-safe: touches only the immutable `client` and the request's
    /// own captured document. Callers run this off the main thread.
    func searchResult(for request: SearchRequest) -> Result<[SearchMatch], Error> {
        Result { try client.search(document: request.document, query: request.query) }
    }

    func applySearch(_ result: Result<[SearchMatch], Error>, for request: SearchRequest) {
        // The document was replaced (or reopened) while the search ran: its
        // matches would address pages that are no longer on screen.
        guard request.generation == generation else { return }
        switch result {
        case let .success(matches) where matches.isEmpty:
            search = nil
            searchStatus = "No matches for \"\(request.query)\"."
        case let .success(matches):
            search = SearchSession(query: request.query, matches: matches, currentIndex: 0)
            searchStatus = Self.searchStatusText(query: request.query, index: 0, total: matches.count)
        case let .failure(error):
            search = nil
            searchStatus = "Could not search: \(Self.message(for: error))"
        }
    }

    func clearSearch() {
        search = nil
        searchStatus = ""
    }

    /// Steps to the next/previous match with wraparound: `Next` on the last
    /// match returns to the first.
    func stepSearchMatch(by delta: Int) {
        guard var session = search, !session.matches.isEmpty else { return }
        let count = session.matches.count
        session.currentIndex = ((session.currentIndex + delta) % count + count) % count
        search = session
        searchStatus = Self.searchStatusText(query: session.query, index: session.currentIndex, total: count)
    }

    /// The page the current match is on, so a view can scroll it into view.
    var currentSearchMatchPage: Int? {
        guard let search, search.matches.indices.contains(search.currentIndex) else { return nil }
        return search.matches[search.currentIndex].pageIndex
    }

    /// Every match's highlight rects on `page`, excluding the current one —
    /// callers pair this with `currentSearchMatchRects` to paint the current
    /// match distinctly.
    func searchMatchRects(forPage page: Int) -> [TextRect] {
        guard let search else { return [] }
        return search.matches.enumerated()
            .filter { $0.offset != search.currentIndex && $0.element.pageIndex == page }
            .flatMap { $0.element.characterBounds }
    }

    func currentSearchMatchRects(forPage page: Int) -> [TextRect] {
        guard let search, search.matches.indices.contains(search.currentIndex),
              search.matches[search.currentIndex].pageIndex == page
        else { return [] }
        return search.matches[search.currentIndex].characterBounds
    }

    private static func searchStatusText(query: String, index: Int, total: Int) -> String {
        "Match \(index + 1) of \(total) for \"\(query)\"."
    }

    private static func message(for error: Error) -> String {
        (error as? LocalizedError)?.errorDescription ?? String(describing: error)
    }
}
