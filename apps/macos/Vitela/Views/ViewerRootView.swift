import SwiftUI

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""
    @State private var searchQuery = ""
    @FocusState private var searchFieldFocused: Bool

    private static let zoomStep = 0.1

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            searchBar
            Divider()
            if model.store.pageSlots.isEmpty {
                placeholder
            } else {
                pageList
            }
        }
        .accessibilityIdentifier("viewer-root")
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
            Button("Copy", action: model.copySelection)
                .keyboardShortcut("c", modifiers: .command)
                .disabled(model.store.selectedText == nil)
            Spacer()
            Button("−") { model.store.setZoom(model.store.zoom - Self.zoomStep) }
                .accessibilityLabel("Zoom out")
            Text("\(Int((model.store.zoom * 100).rounded()))%")
                // Not `.monospacedDigit()`: that is macOS 12+, and the
                // deployment floor is 11.0. A monospaced face keeps the
                // readout from jittering as the digit count changes.
                .font(.system(.body, design: .monospaced))
            Button("+") { model.store.setZoom(model.store.zoom + Self.zoomStep) }
                .accessibilityLabel("Zoom in")
        }
        .padding()
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
        }
        .padding(.horizontal)
        .padding(.bottom, 8)
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
                .padding()
            }
            .onChange(of: model.store.currentSearchMatchPage) { page in
                guard let page else { return }
                withAnimation { proxy.scrollTo(pageId(page: page), anchor: .top) }
            }
        }
    }

    private func pageId(_ slot: PageSlot) -> String { pageId(page: slot.index) }

    private func pageId(page: Int) -> String { "\(model.store.generation)-\(page)" }

    private func pageWidth(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.width * model.store.zoom) }

    private func pageHeight(_ slot: PageSlot) -> CGFloat { CGFloat(slot.dimensions.height * model.store.zoom) }
}
