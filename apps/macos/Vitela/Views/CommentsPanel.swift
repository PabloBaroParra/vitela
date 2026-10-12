import SwiftUI

/// The document's comments, by page — Windows' Comments tool page. Each one
/// opens read-only; a note this session added is selected as it opens.
struct CommentsPanel: View {
    @ObservedObject var store: ViewerStore
    @State private var reading: Comment?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Comments").font(.headline)
            if store.comments.isEmpty {
                Text("No comments in this document.").foregroundStyle(.secondary)
            }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 8) {
                    ForEach(store.commentsByPage, id: \.page) { group in
                        Text("Page \(group.page + 1)").fontWeight(.semibold)
                        ForEach(Array(group.comments.enumerated()), id: \.offset) { _, comment in
                            Button { open(comment) } label: { row(comment) }
                                .buttonStyle(.plain)
                        }
                    }
                }
            }
        }
        .padding()
        .accessibilityIdentifier("comments-panel")
        .sheet(item: readingBinding) { item in
            CommentSheet(comment: item.comment) { reading = nil }
        }
    }

    private func row(_ comment: Comment) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(comment.contents).lineLimit(3)
            if !comment.byline.isEmpty {
                Text(comment.byline).font(.caption).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(8)
        .background(RoundedRectangle(cornerRadius: 6).fill(Color(nsColor: .controlBackgroundColor)))
    }

    private func open(_ comment: Comment) {
        store.revealComment(comment)
        reading = comment
    }

    private var readingBinding: Binding<ReadComment?> {
        Binding(get: { reading.map(ReadComment.init) }, set: { reading = $0?.comment })
    }
}

/// A comment as a sheet item.
private struct ReadComment: Identifiable {
    let comment: Comment
    var id: String { "\(comment.page)-\(comment.rect.x)-\(comment.rect.y)-\(comment.contents.hashValue)" }
}

/// One comment, read-only — Windows' "Comment — page N" dialog.
private struct CommentSheet: View {
    let comment: Comment
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Comment — page \(comment.page + 1)").font(.headline)
            if !comment.byline.isEmpty {
                Text(comment.byline).foregroundStyle(.secondary)
            }
            ScrollView {
                Text(comment.contents)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(minWidth: 300, minHeight: 120, maxHeight: 300)
            HStack {
                Spacer()
                Button("Close", action: close).keyboardShortcut(.defaultAction)
            }
        }
        .padding()
    }
}
