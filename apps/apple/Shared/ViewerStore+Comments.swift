// The Comments list: what the file's reviewers left, plus this session's
// notes, grouped by page. Mirrors Windows' `MainWindow.Comments.cs`; the core
// owns the merged snapshot (`DocumentHandle.comments`).
import Foundation

extension ViewerStore {
    /// Comments grouped by page, pages in order.
    var commentsByPage: [(page: Int, comments: [Comment])] {
        Dictionary(grouping: comments, by: \.page)
            .sorted { $0.key < $1.key }
            .map { (page: $0.key, comments: $0.value) }
    }

    /// Reading a comment brings its page into view, as Windows does, and
    /// selects its note when it is one this session added, so its handles
    /// show and Delete/Read Note apply to it.
    func revealComment(_ comment: Comment) {
        goToPage(comment.page)
        guard let id = comment.annotationId, annotations.contains(where: { $0.id == id }) else { return }
        selectedAnnotationId = id
    }
}
