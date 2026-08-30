import SwiftUI
import UniformTypeIdentifiers

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""
    @State private var searchQuery = ""
    @FocusState private var searchFieldFocused: Bool

    private static let zoomStep = 0.1

    var body: some View {
        NavigationView {
            VStack(spacing: 0) {
                searchBar
                Divider()
                content
            }
                .navigationBarTitleDisplayMode(.inline)
                .navigationTitle(model.title)
                .toolbar {
                    // A single Menu rather than two separate buttons: with
                    // "Open PDF" and "Open Sample" as their own toolbar items
                    // plus the three trailing zoom controls, the bar had more
                    // items than fit and the system started collapsing the
                    // trailing group behind an overflow "…" button, burying
                    // zoom in an extra tap.
                    ToolbarItem(placement: .navigationBarLeading) {
                        Menu {
                            Button("Open PDF", action: model.selectDocument)
                            Menu("Open Sample") {
                                Button("Vitela sample", action: model.openSample)
                                Button("AES-128 sample (user-aes-pass)", action: model.openAes128Sample)
                                Button("RC4-128 sample (user-rc4-pass)", action: model.openRc4128Sample)
                            }
                        } label: {
                            Label("Open", systemImage: "doc.badge.plus")
                        }
                    }
                    // A separate leading item rather than a fourth member of
                    // the trailing zoom group: that group already fills the
                    // bar's trailing edge on an iPhone in portrait, and a
                    // fourth item there squeezed the percentage readout down
                    // to "10…" instead of "100%" — the same overflow this
                    // file already consolidated the leading menu to avoid.
                    ToolbarItem(placement: .navigationBarLeading) {
                        Button(action: model.copySelection) {
                            Label("Copy", systemImage: "doc.on.doc")
                        }
                        .keyboardShortcut("c", modifiers: .command)
                        .disabled(model.store.selectedText == nil)
                    }
                    ToolbarItemGroup(placement: .navigationBarTrailing) {
                        Button("−") { model.store.setZoom(model.store.zoom - Self.zoomStep) }
                            .accessibilityLabel("Zoom out")
                        Text("\(Int((model.store.zoom * 100).rounded()))%")
                            // A monospaced face keeps the readout from jittering
                            // as the digit count changes.
                            .font(.system(.body, design: .monospaced))
                        Button("+") { model.store.setZoom(model.store.zoom + Self.zoomStep) }
                            .accessibilityLabel("Zoom in")
                    }
                }
        }
        // iPad would otherwise get the split-view style and render the viewer
        // in a collapsed sidebar column.
        .navigationViewStyle(.stack)
        .fileImporter(
            isPresented: $model.isSelectingDocument,
            allowedContentTypes: [.pdf],
            allowsMultipleSelection: false,
            onCompletion: model.finishSelection
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
        .accessibilityIdentifier("viewer-root")
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
        VStack(alignment: .leading, spacing: 4) {
            HStack {
                TextField("Find in document", text: $searchQuery)
                    .textFieldStyle(.roundedBorder)
                    .submitLabel(.search)
                    .autocapitalization(.none)
                    .disableAutocorrection(true)
                    .focused($searchFieldFocused)
                    .onSubmit { model.runSearch(searchQuery) }
                Button("Previous") { model.stepSearchMatch(by: -1) }
                    .disabled(model.store.search == nil)
                Button("Next") { model.stepSearchMatch(by: 1) }
                    .disabled(model.store.search == nil)
            }
            if !model.store.searchStatus.isEmpty {
                Text(model.store.searchStatus)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal)
        .padding(.vertical, 8)
    }

    @ViewBuilder
    private var content: some View {
        if model.store.pageSlots.isEmpty {
            placeholder
        } else {
            pageList
        }
    }

    private var placeholder: some View {
        VStack(spacing: 12) {
            AppMarkView()
            Text(model.title)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var pageList: some View {
        GeometryReader { geometry in
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 16) {
                        ForEach(model.store.pageSlots, id: \.index) { slot in
                            ZStack(alignment: .topLeading) {
                                PageView(slot: slot, zoom: model.store.zoom) { model.retry(page: slot.index) }
                                    // `render` is a no-op when the slot is already
                                    // current, so re-appearing during scroll costs
                                    // nothing. A zoom change makes every visible
                                    // slot stale, which is what re-renders them at
                                    // the new DPI instead of upscaling.
                                    .onAppear { model.render(page: slot.index) }
                                    .onChange(of: model.store.zoom) { _ in model.render(page: slot.index) }
                                SelectionOverlay(
                                    dimensions: slot.dimensions,
                                    zoom: model.store.zoom,
                                    selectionRects: model.store.selectionRects(forPage: slot.index),
                                    matchRects: model.store.searchMatchRects(forPage: slot.index),
                                    currentMatchRects: model.store.currentSearchMatchRects(forPage: slot.index),
                                    onDragBegan: { location in
                                        model.beginSelection(
                                            page: slot.index, location: location,
                                            dimensions: slot.dimensions, zoom: model.store.zoom
                                        )
                                    },
                                    onDragChanged: { location in
                                        model.extendSelection(
                                            page: slot.index, location: location,
                                            dimensions: slot.dimensions, zoom: model.store.zoom
                                        )
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
                        }
                    }
                    .padding(Self.pageListPadding)
                }
                // `zoom` is a raw scale (1.0 == 1 PDF point per screen point), not
                // a fit-to-width factor. A US Letter/A4 page is ~612pt wide, well
                // past an iPhone's screen width, so without this a freshly opened
                // document renders wider than the screen and the default zoom
                // starts the reader clipped until the user zooms out by hand.
                .onChange(of: model.store.generation) { _ in fitToWidth(availableWidth: geometry.size.width) }
                .onAppear { fitToWidth(availableWidth: geometry.size.width) }
                .onChange(of: model.store.currentSearchMatchPage) { page in
                    guard let page else { return }
                    withAnimation { proxy.scrollTo(pageId(page: page), anchor: .top) }
                }
            }
        }
    }

    private func pageId(_ slot: PageSlot) -> String { pageId(page: slot.index) }

    private func pageId(page: Int) -> String { "\(model.store.generation)-\(page)" }

    private func pageWidth(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.width * model.store.zoom) }

    private func pageHeight(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.height * model.store.zoom) }

    private static let pageListPadding: CGFloat = 16

    /// Sets zoom so the first page's width (all pages share a width in
    /// practice) fills the available scroll width, run once per opened
    /// document rather than continuously, so it does not fight a zoom the
    /// user set by hand.
    private func fitToWidth(availableWidth: CGFloat) {
        guard let firstPageWidth = model.store.pageSlots.first?.dimensions.width, firstPageWidth > 0 else { return }
        let target = (availableWidth - Self.pageListPadding * 2) / CGFloat(firstPageWidth)
        model.store.setZoom(Double(target))
    }
}
