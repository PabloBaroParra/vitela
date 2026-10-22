import SwiftUI
import UniformTypeIdentifiers

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""
    @State private var searchQuery = ""
    @FocusState private var searchFieldFocused: Bool
    @Environment(\.displayScale) private var displayScale
    /// The page scroller's own size, which bounds the part of a page in view.
    @State private var scrollSize = CGSize.zero
    /// The page column's least width, so narrower pages stay centred.
    @State private var pageColumnWidth = 0.0
    @State private var showsProperties = false
    @State private var showsComments = false
    @State private var showsForm = false
    // Each side column's width, dragged from its divider (`SideColumn`).
    // Pages starts at the 136 it always had and keeps Windows' 120–320;
    // the right-hand panels start at their old fixed widths.
    @State private var pagesColumn = SideColumn(minimum: 120, maximum: 320, initial: 136)
    @State private var searchColumn = SideColumn(minimum: 200, maximum: 600, initial: 260)
    @State private var propertiesColumn = SideColumn(minimum: 220, maximum: 600, initial: 280)
    @State private var commentsColumn = SideColumn(minimum: 220, maximum: 600, initial: 280)
    @State private var formColumn = SideColumn(minimum: 220, maximum: 600, initial: 280)

    var body: some View {
        VStack(spacing: 0) {
            ViewerToolbar(model: model, showsProperties: $showsProperties, showsComments: $showsComments, showsForm: $showsForm)
            searchBar
            AnnotationToolbar(store: model.store, drawSignature: model.drawSignature, organizing: model.isOrganizing)
            Divider()
            HStack(spacing: 0) {
                if !model.store.pageSlots.isEmpty && !model.showsHome {
                    if model.showsPages {
                        PagesSidebar(model: model)
                            .frame(width: pagesColumn.preferredWidth)
                    }
                    // Stays while Pages is folded, so dragging it back out
                    // reopens the column — as on Linux and Windows.
                    ColumnDivider(column: $pagesColumn, isOpen: $model.showsPages, widening: 1,
                                  label: "Resize the Pages column")
                }
                if model.store.pageSlots.isEmpty || model.showsHome {
                    placeholder
                } else if model.isOrganizing {
                    OrganizeView(model: model)
                } else {
                    pageList
                }
                if model.store.search != nil {
                    // Results close with the search, not with a drag: this
                    // divider only resizes.
                    ColumnDivider(column: $searchColumn, isOpen: .constant(true), widening: -1,
                                  label: "Resize the search results")
                    SearchResultsPanel(store: model.store).frame(width: searchColumn.preferredWidth)
                }
                if showsProperties {
                    ColumnDivider(column: $propertiesColumn, isOpen: $showsProperties, widening: -1,
                                  label: "Resize the Properties panel")
                    PropertiesPanel(store: model.store)
                        .frame(width: propertiesColumn.preferredWidth)
                }
                if showsComments {
                    ColumnDivider(column: $commentsColumn, isOpen: $showsComments, widening: -1,
                                  label: "Resize the Comments panel")
                    CommentsPanel(store: model.store)
                        .frame(width: commentsColumn.preferredWidth)
                }
                if showsForm {
                    ColumnDivider(column: $formColumn, isOpen: $showsForm, widening: -1,
                                  label: "Resize the Form panel")
                    FormFieldsPanel(model: model)
                        .frame(width: formColumn.preferredWidth)
                }
            }
            if !statusText.isEmpty {
                Divider()
                statusLine
            }
        }
        .accessibilityIdentifier("viewer-root")
        // Controls that take a tint (prominent buttons, switches, progress)
        // wear Vitela's accent, as on Linux and Windows.
        .tint(.vitelaAccent)
        // The whole window takes the drop, so a PDF can be dropped with
        // nothing open and no page to aim at.
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            Self.loadFileURLs(providers, then: model.openExternalFiles)
            return !providers.isEmpty
        }
        .onAppear(perform: model.installAnnotationKeyMonitor)
        .background(
            // `.hidden()` can pull a view out of the responder chain on some
            // macOS versions, which would silently kill the shortcut;
            // `.opacity(0)` keeps it live while staying invisible.
            Button("Find") { searchFieldFocused = true }
                .keyboardShortcut("f", modifiers: .command)
                .opacity(0)
        )
        .alert("Password required", isPresented: isPresentingPasswordPrompt) {
            SecureField("Password", text: $passwordInput)
            Button("Open") {
                model.submitPassword(passwordInput)
                passwordInput = ""
            }
            Button("Cancel", role: .cancel) { passwordInput = "" }
        } message: {
            if case .error(.wrongPassword) = model.store.state {
                Text(ViewerFailure.wrongPassword.message)
            }
        }
        .modifier(ViewerSheets(model: model))
        .onChange(of: model.store.pendingStamp) { pending in
            // After this update, so the open panel never runs mid-gesture.
            if pending != nil { DispatchQueue.main.async(execute: model.resolvePendingStamp) }
        }
    }

    /// True while the store is waiting on a password — covers both the first
    /// ask (`.passwordRequired`) and a retry after a wrong one
    /// (`.wrongPassword`), so the prompt reappears until the user cancels or
    /// gets it right.
    private var isPresentingPasswordPrompt: Binding<Bool> {
        Binding(
            get: {
                switch model.store.state {
                case .error(.passwordRequired), .error(.wrongPassword): return true
                default: return false
                }
            },
            set: { _ in }
        )
    }

    private var searchBar: some View {
        HStack {
            TextField("Find in document", text: $searchQuery)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 240)
                .focused($searchFieldFocused)
                .onSubmit { model.runSearch(searchQuery) }
            Button { model.runSearch(searchQuery) } label: { Label("Find in document", shellIcon: .search) }
                .labelStyle(.iconOnly)
                .help("Find in document (⌘F)")
            Button { model.stepSearchMatch(by: -1) } label: { Label("Previous match", shellIcon: .previous) }
                .labelStyle(.iconOnly)
                .help("Previous match")
                .disabled(model.store.search == nil)
            Button { model.stepSearchMatch(by: 1) } label: { Label("Next match", shellIcon: .next) }
                .labelStyle(.iconOnly)
                .help("Next match")
                .disabled(model.store.search == nil)
            if !model.store.searchStatus.isEmpty {
                Text(model.store.searchStatus)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            PageNavigationBar(model: model)
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
    }

    /// The shell's own notice wins: it answers what the reader just did.
    private var statusText: String {
        model.notice.isEmpty ? model.store.editStatus : model.notice
    }

    /// Resolves the dropped items' file URLs, then hands them over on main.
    static func loadFileURLs(_ providers: [NSItemProvider], then deliver: @escaping ([URL]) -> Void) {
        var urls = [URL?](repeating: nil, count: providers.count)
        let group = DispatchGroup()
        for (index, provider) in providers.enumerated() {
            group.enter()
            _ = provider.loadObject(ofClass: URL.self) { url, _ in
                DispatchQueue.main.async {
                    urls[index] = url
                    group.leave()
                }
            }
        }
        group.notify(queue: .main) { deliver(urls.compactMap { $0 }) }
    }

    private var statusLine: some View {
        HStack {
            Text(statusText)
                .foregroundStyle(.secondary)
                .accessibilityIdentifier("edit-status")
            Spacer()
            if model.store.hasUnsavedChanges {
                Text("Unsaved changes").foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 6)
    }

    private var placeholder: some View {
        HomeView(model: model)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var pageList: some View {
        ScrollViewReader { proxy in
            // Both axes: a page zoomed wider than the window pans sideways
            // instead of being cut off at the window's edges.
            ScrollView([.horizontal, .vertical]) {
                LazyVStack(spacing: 16) {
                    ForEach(model.store.pageSlots, id: \.index) { slot in
                        ZStack(alignment: .topLeading) {
                            PageView(slot: slot, zoom: pageZoom(slot)) { model.retry(page: slot.index) }
                                // `render` is a no-op when the slot is already
                                // current, so re-appearing during scroll costs
                                // nothing. A zoom change — or a move to a
                                // screen of another backing scale — makes every
                                // visible slot stale, which is what re-renders
                                // them at the new DPI instead of stretching.
                                .onAppear { model.render(page: slot.index, scale: Double(displayScale)) }
                                .onChange(of: pageZoom(slot)) { _ in model.render(page: slot.index, scale: Double(displayScale)) }
                                .onChange(of: displayScale) { model.render(page: slot.index, scale: Double($0)) }
                            PageTilesView(tiles: model.store.pageTiles[slot.index], zoom: pageZoom(slot))
                                .frame(width: pageWidth(slot), height: pageHeight(slot))
                            AnnotationOverlay(store: model.store, page: slot.index, zoom: pageZoom(slot))
                                .frame(width: pageWidth(slot), height: pageHeight(slot))
                            NoteTooltipLayer(tooltips: model.store.noteTooltips(page: slot.index, zoom: pageZoom(slot)))
                                .frame(width: pageWidth(slot), height: pageHeight(slot))
                            ContentOutlineOverlay(store: model.store, page: slot.index, zoom: pageZoom(slot)) {
                                model.readPageContent(page: slot.index)
                            }
                            .frame(width: pageWidth(slot), height: pageHeight(slot))
                            SelectionOverlay(
                                dimensions: slot.dimensions,
                                zoom: pageZoom(slot),
                                selectionRects: model.store.selectionRects(forPage: slot.index),
                                matchRects: model.store.searchMatchRects(forPage: slot.index),
                                currentMatchRects: model.store.currentSearchMatchRects(forPage: slot.index),
                                onDragBegan: { location in
                                    model.pageDragBegan(
                                        page: slot.index, location: location,
                                        dimensions: slot.dimensions, zoom: pageZoom(slot)
                                    )
                                },
                                onDragChanged: { location in
                                    model.pageDragChanged(
                                        page: slot.index, location: location,
                                        dimensions: slot.dimensions, zoom: pageZoom(slot)
                                    )
                                },
                                onDragEnded: { location in
                                    model.pageDragEnded(page: slot.index, location: location, zoom: pageZoom(slot))
                                }
                            )
                            .frame(width: pageWidth(slot), height: pageHeight(slot))
                        }
                        // `pageSlots` reuses the same `index` values across
                        // documents, so without a generation-keyed identity
                        // here `ForEach` would treat a newly opened document's
                        // rows as the *same* views as the previous document's:
                        // `onAppear` would never refire, and every page would
                        // stay stuck on its "Rendering…" placeholder forever.
                        // A file dropped on a page lands there: an image as
                        // a stamp at the drop point, a PDF opens.
                        .onDrop(of: [.fileURL], isTargeted: nil) { providers, location in
                            Self.loadFileURLs(providers) {
                                model.dropFiles($0, onPage: slot.index, at: location, zoom: pageZoom(slot))
                            }
                            return !providers.isEmpty
                        }
                        .id(pageId(slot))
                        .background(pageSpanReader(slot.index))
                        // Past the bitmap's pixel budget the part in view is
                        // re-rendered sharp, in tiles; off screen it keeps none.
                        .background(PageViewRectReader(space: Self.pageScrollSpace, viewport: scrollSize, zoom: pageZoom(slot)) {
                            model.renderTiles(page: slot.index, visible: $0)
                        })
                        .onDisappear { [generation = model.store.generation] in
                            model.store.forgetTiles(page: slot.index)
                            model.pageLeftView(slot.index, generation: generation)
                        }
                    }
                }
                .padding(ViewerViewModel.pageListPadding)
                .frame(minWidth: pageColumnWidth)
            }
            .coordinateSpace(name: Self.pageScrollSpace)
            // The recessed surface the pages sit on — Linux's `vitela_canvas`
            // behind `.canvas-frame`, Windows' `VitelaCanvasBackgroundBrush`.
            .background(Color.vitela(.canvas))
            .onPreferenceChange(PageSpansKey.self) { spans in
                model.store.updateCurrentPage(spans: spans)
                model.store.releasePages(farFrom: spans)
            }
            .background(GeometryReader { geometry in
                Color.clear
                    .onAppear { scrollSizeChanged(geometry.size) }
                    .onChange(of: geometry.size, perform: scrollSizeChanged)
            })
            .onChange(of: model.store.currentSearchMatchPage) { page in
                guard let page else { return }
                withAnimation { proxy.scrollTo(pageId(page: page), anchor: .top) }
            }
            .onChange(of: model.store.pageNavigation) { request in
                guard let request else { return }
                proxy.scrollTo(pageId(page: request.page), anchor: request.anchor.map { UnitPoint(x: $0.x, y: $0.y) } ?? .top)
                // The layout passes `scrollTo` triggers run before the next
                // main-queue turn; after it, a moving page is the reader.
                DispatchQueue.main.async { model.store.pageNavigationLanded(request) }
            }
        }
    }

    private static let pageScrollSpace = "page-scroll"

    /// Reports where a page row sits relative to the scroll view's visible
    /// area, so the store can tell which page is current.
    private func scrollSizeChanged(_ size: CGSize) {
        scrollSize = size
        pageColumnWidth = ViewerViewModel.pageColumnMinimumWidth(for: size)
        model.updateViewport(size)
    }

    private func pageSpanReader(_ page: Int) -> some View {
        GeometryReader { geometry in
            let frame = geometry.frame(in: .named(Self.pageScrollSpace))
            Color.clear.preference(
                key: PageSpansKey.self,
                value: [page: PageViewportSpan(top: Double(frame.minY), bottom: Double(frame.maxY))]
            )
        }
    }

    private func pageId(_ slot: PageSlot) -> String { pageId(page: slot.index) }

    private func pageId(page: Int) -> String { "\(model.store.generation)-\(page)" }

    /// The factor this page is shown at — per page, because a fit mode
    /// resolves each page against the viewport on its own.
    private func pageZoom(_ slot: PageSlot) -> Double { model.store.zoom(forPage: slot.index) }

    private func pageWidth(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.width * pageZoom(slot)) }

    private func pageHeight(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.height * pageZoom(slot)) }
}

/// Collects every laid-out page row's `PageViewportSpan` in one dictionary.
private struct PageSpansKey: PreferenceKey {
    static var defaultValue: [Int: PageViewportSpan] = [:]

    static func reduce(value: inout [Int: PageViewportSpan], nextValue: () -> [Int: PageViewportSpan]) {
        value.merge(nextValue()) { _, new in new }
    }
}
