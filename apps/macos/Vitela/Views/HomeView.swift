import SwiftUI

/// What the window shows with no document open: the mark, Open and New, the
/// Recent list grouped by day, the Tools column (`HomeToolsColumn`) and the
/// build's version (`AppVersion`) —
/// Linux's Home (`home/`) and Windows' `MainWindow.Home.cs` /
/// `MainWindow.Recents.cs`. The rules come from
/// `RecentDocuments`; opening, removing and the previews from
/// `ViewerViewModel+Recents.swift`.
struct HomeView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var query = ""
    /// The Recent card with the keyboard (Full Keyboard Access), so Delete
    /// knows which card it removes and where the focus goes next.
    @FocusState private var focusedCard: String?
    @Environment(\.displayScale) private var displayScale

    /// The preview frame, which also sets the card's text width — Windows'
    /// and Linux's 108.
    static let cardWidth = 108.0

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                header
                HStack(alignment: .top, spacing: 24) {
                    VStack(alignment: .leading, spacing: 20) {
                        recentHeading
                        recentList
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    HomeToolsColumn(model: model, query: query)
                }
                Text(AppVersion.footerText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .trailing)
            }
            .padding(24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .onAppear { model.loadRecentPreviews(scale: Double(displayScale), retry: true) }
        .onChange(of: model.recentDocuments) { _ in model.loadRecentPreviews(scale: Double(displayScale)) }
    }

    private var header: some View {
        HStack(spacing: 16) {
            AppMarkView()
            VStack(alignment: .leading, spacing: 6) {
                Text("Welcome to Vitela").font(.largeTitle)
                Text("Your fast, private PDF workspace.").foregroundStyle(.secondary)
                Text(model.title).foregroundStyle(.secondary)
            }
            Spacer()
            if model.showsHome {
                Button("Return to Document", action: model.returnToDocument)
                    .help("Back to the open document, as you left it (⇧⌘H)")
            }
            Button("New", action: model.newDocument)
            Button("Open…", action: model.selectDocument)
                .keyboardShortcut(.defaultAction)
        }
    }

    private var recentHeading: some View {
        HStack {
            Text("Recent").font(.title2)
            Spacer()
            // One field filtering Recent and the Tools grid, Linux's "Search
            // files and tools" and Windows' `HomeSearchBox`.
            TextField("Search files and tools", text: $query)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 220)
        }
    }

    @ViewBuilder private var recentList: some View {
        let groups = RecentDocuments.grouped(model.recentDocuments, matching: query, now: Date())
        if groups.isEmpty {
            Text(RecentDocuments.emptyMessage(listIsEmpty: model.recentDocuments.isEmpty))
                .foregroundStyle(.secondary)
        }
        ForEach(groups, id: \.bucket) { group in
            VStack(alignment: .leading, spacing: 8) {
                Text(group.bucket.rawValue)
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(Capsule().fill(Color.secondary.opacity(0.15)))
                LazyVGrid(columns: [GridItem(.adaptive(minimum: Self.cardWidth + 16), spacing: 10, alignment: .top)],
                          alignment: .leading, spacing: 10) {
                    ForEach(group.documents, id: \.path) { card($0) }
                }
            }
        }
    }

    private func card(_ document: RecentDocument) -> some View {
        let preview = model.recentPreviews[document.path]
        return Button { model.openRecent(document) } label: {
            VStack(alignment: .leading, spacing: 6) {
                picture(preview?.image)
                    .frame(width: Self.cardWidth, height: Self.cardWidth)
                Text(document.name)
                    .lineLimit(2)
                    .font(.callout)
                Text(RecentDocuments.caption(document, pageCount: preview?.pageCount, now: Date()))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .frame(width: Self.cardWidth, alignment: .leading)
            .padding(8)
            .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: .controlBackgroundColor)))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help(document.path)
        .accessibilityLabel("Open \(document.name)")
        .focused($focusedCard, equals: document.path)
        // Delete removes the focused card, as on Linux and Windows; the
        // keyboard then lands on the newest card left rather than on nothing.
        // The context menu's removal leaves the focus alone, as theirs does.
        .onDeleteCommand {
            model.removeRecent(document)
            focusedCard = RecentDocuments.firstShown(model.recentDocuments, matching: query, now: Date())
        }
        .contextMenu {
            Button("Remove from Recent") { model.removeRecent(document) }
        }
    }

    @ViewBuilder private func picture(_ image: RenderedPage?) -> some View {
        if let image = image?.nsImage {
            Image(nsImage: image)
                .resizable()
                .aspectRatio(contentMode: .fit)
                .background(Color.white)
                .shadow(radius: 1)
        } else {
            // Passwords, damaged files and pages still rendering: a blank
            // frame, as on Windows.
            RoundedRectangle(cornerRadius: 4).fill(Color.secondary.opacity(0.1))
        }
    }
}
