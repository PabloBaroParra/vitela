import SwiftUI
import UniformTypeIdentifiers

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""
    @State private var searchQuery = ""
    @FocusState private var searchFieldFocused: Bool
    @State private var showsProperties = false

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            searchBar
            AnnotationToolbar(store: model.store)
            Divider()
            HStack(spacing: 0) {
                if model.store.pageSlots.isEmpty {
                    placeholder
                } else if model.isOrganizing {
                    OrganizeView(model: model)
                } else {
                    pageList
                }
                if showsProperties {
                    Divider()
                    PropertiesPanel(store: model.store)
                        .frame(width: 280)
                }
            }
            if !statusText.isEmpty {
                Divider()
                statusLine
            }
        }
        .accessibilityIdentifier("viewer-root")
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

    private var toolbar: some View {
        HStack {
            Button("Open PDF", action: model.selectDocument)
            Menu("Open Sample") {
                Button("Vitela sample", action: model.openSample)
                Button("AES-128 sample (user-aes-pass)", action: model.openAes128Sample)
                Button("RC4-128 sample (user-rc4-pass)", action: model.openRc4128Sample)
            }
            Button("Save As…") { model.saveAs() }
                .disabled(model.store.document == nil || model.isSaving)
            Button("Print…", action: model.printDocument)
                .disabled(model.store.document == nil)
            Button("Copy", action: model.copySelection)
                .keyboardShortcut("c", modifiers: .command)
                .disabled(model.store.selectedText == nil)
            Button("Undo", action: model.undo)
                .disabled(!model.store.canUndo || model.store.pageWorkInFlight)
            Button("Redo", action: model.redo)
                .disabled(!model.store.canRedo || model.store.pageWorkInFlight)
            Toggle("Properties", isOn: $showsProperties)
                .toggleStyle(.button)
            if model.store.isSigned {
                Text("✓ Digitally signed")
                    .foregroundStyle(.secondary)
                    .help("This document carries a digital signature.")
                    .accessibilityIdentifier("signed-indicator")
            }
            Toggle("Organize", isOn: $model.isOrganizing)
                .toggleStyle(.button)
                .help("Rotate and rearrange pages")
                .disabled(model.store.document == nil)
            Spacer()
            Button("−") { model.store.stepZoom(-1) }
                .accessibilityLabel("Zoom out")
                .help("Zoom out (⌘−)")
            Text("\(Int((model.store.displayedZoom * 100).rounded()))%")
                // A monospaced face keeps the readout from jittering as the
                // digit count changes.
                .font(.system(.body, design: .monospaced))
            Button("+") { model.store.stepZoom(1) }
                .accessibilityLabel("Zoom in")
                .help("Zoom in (⌘=)")
            Toggle("Fit width", isOn: zoomModeBinding(.fitWidth))
                .toggleStyle(.button)
                .help("Fit each page to the window width")
            Toggle("Fit page", isOn: zoomModeBinding(.fitPage))
                .toggleStyle(.button)
                .help("Fit each whole page in the window (⌘9)")
        }
        .padding()
    }

    /// On while `mode` is active; switching it off freezes the factor the
    /// pages are shown at, as an explicit zoom.
    private func zoomModeBinding(_ mode: ZoomMode) -> Binding<Bool> {
        Binding(
            get: { model.store.zoomMode == mode },
            set: { model.store.setZoomMode($0 ? mode : .custom) }
        )
    }

    private var searchBar: some View {
        HStack {
            TextField("Find in document", text: $searchQuery)
                .textFieldStyle(.roundedBorder)
                .frame(maxWidth: 240)
                .focused($searchFieldFocused)
                .onSubmit { model.runSearch(searchQuery) }
            Button("Previous") { model.stepSearchMatch(by: -1) }
                .disabled(model.store.search == nil)
            Button("Next") { model.stepSearchMatch(by: 1) }
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
    private static func loadFileURLs(_ providers: [NSItemProvider], then deliver: @escaping ([URL]) -> Void) {
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
        VStack(spacing: 12) {
            AppMarkView()
            Text(model.title)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var pageList: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 16) {
                    ForEach(model.store.pageSlots, id: \.index) { slot in
                        ZStack(alignment: .topLeading) {
                            PageView(slot: slot, zoom: pageZoom(slot)) { model.retry(page: slot.index) }
                                // `render` is a no-op when the slot is already
                                // current, so re-appearing during scroll costs
                                // nothing. A zoom change makes every visible
                                // slot stale, which is what re-renders them at
                                // the new DPI instead of upscaling.
                                .onAppear { model.render(page: slot.index) }
                                .onChange(of: pageZoom(slot)) { _ in model.render(page: slot.index) }
                            AnnotationOverlay(store: model.store, page: slot.index, zoom: pageZoom(slot))
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
                        .id(pageId(slot))
                        .background(pageSpanReader(slot.index))
                    }
                }
                .padding(ViewerViewModel.pageListPadding)
            }
            .coordinateSpace(name: Self.pageScrollSpace)
            .onPreferenceChange(PageSpansKey.self) { model.store.updateCurrentPage(spans: $0) }
            .background(GeometryReader { geometry in
                Color.clear
                    .onAppear { model.updateViewport(geometry.size) }
                    .onChange(of: geometry.size) { model.updateViewport($0) }
            })
            .onChange(of: model.store.currentSearchMatchPage) { page in
                guard let page else { return }
                withAnimation { proxy.scrollTo(pageId(page: page), anchor: .top) }
            }
            .onChange(of: model.store.pageNavigation) { request in
                guard let request else { return }
                proxy.scrollTo(pageId(page: request.page), anchor: .top)
                // The layout passes `scrollTo` triggers run before the next
                // main-queue turn; after it, a moving page is the reader.
                DispatchQueue.main.async { model.store.pageNavigationLanded(request) }
            }
        }
    }

    private static let pageScrollSpace = "page-scroll"

    /// Reports where a page row sits relative to the scroll view's visible
    /// area, so the store can tell which page is current.
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
