import SwiftUI

/// What the window shows on Home: Open and New, the
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

    /// Home's one responsive rule — Windows' `HomeLayout`: the Tools column,
    /// Linux's `SIDE_COLUMN_WIDTH` wide, sits beside Start and Recent while
    /// the body has room for both, and drops under them otherwise.
    static let toolsColumnWidth: CGFloat = 268
    static let stackBelowWidth = 720.0

    /// An unmeasured body (zero) does not claim room for two columns.
    static func isStacked(bodyWidth: Double) -> Bool { !(bodyWidth >= stackBelowWidth) }

    /// The body's width inside the padding, as last laid out.
    @State private var bodyWidth = 0.0

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    header
                    columns
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
            .onChange(of: model.recentDocuments) { _ in
                model.loadRecentPreviews(scale: Double(displayScale))
                if model.homeSection == .recent && focusedCard == nil { revealRecent(using: proxy) }
            }
            .onAppear { if model.homeSection == .recent { revealRecent(using: proxy) } }
            .onChange(of: model.recentFocusRequest) { _ in revealRecent(using: proxy) }
            .onChange(of: model.homeSection) { section in
                if section == .home { proxy.scrollTo("home-heading", anchor: .top) }
            }
        }
    }

    private func revealRecent(using proxy: ScrollViewProxy) {
        proxy.scrollTo("recent-heading", anchor: .top)
        focusedCard = RecentDocuments.firstShown(model.recentDocuments, matching: query, now: Date())
    }

    @ViewBuilder private var columns: some View {
        Group {
            if Self.isStacked(bodyWidth: bodyWidth) {
                VStack(alignment: .leading, spacing: 20) {
                    startAndRecent
                    HomeToolsColumn(model: model, query: query, width: nil)
                }
            } else {
                HStack(alignment: .top, spacing: 24) {
                    startAndRecent
                    HomeToolsColumn(model: model, query: query, width: Self.toolsColumnWidth)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(GeometryReader { geometry in
            Color.clear
                .onAppear { bodyWidth = geometry.size.width }
                .onChange(of: geometry.size.width) { bodyWidth = $0 }
        })
    }

    private var startAndRecent: some View {
        VStack(alignment: .leading, spacing: 20) {
            Text("Start a new task").font(.title2)
            HomeDropZone(model: model)
            recentHeading
            recentList
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                Text("Welcome to Vitela").font(.largeTitle)
                Text("Your fast, private PDF workspace.").foregroundStyle(.secondary)
                Text(model.title).foregroundStyle(.secondary)
            }
            HStack {
                if model.showsHome {
                    Button("Return to Document", action: model.returnToDocument)
                        .help("Back to the open document, as you left it (⇧⌘H)")
                }
                Button("New", action: model.newDocument)
                Button("Open…", action: model.selectDocument)
                    .keyboardShortcut(.defaultAction)
                Menu("Open Sample") {
                    Button("Vitela sample", action: model.openSample)
                    Button("AES-128 sample (user-aes-pass)", action: model.openAes128Sample)
                    Button("RC4-128 sample (user-rc4-pass)", action: model.openRc4128Sample)
                }
            }
        }
        .id("home-heading")
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
        .id("recent-heading")
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
