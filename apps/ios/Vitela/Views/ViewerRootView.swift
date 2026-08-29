import SwiftUI
import UniformTypeIdentifiers

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel
    @State private var passwordInput = ""

    private static let zoomStep = 0.1

    var body: some View {
        NavigationView {
            content
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
                            Button("Open Sample", action: model.openSample)
                        } label: {
                            Label("Open", systemImage: "doc.badge.plus")
                        }
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
            Image(systemName: "doc.richtext")
                .font(.system(size: 40))
            Text(model.title)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var pageList: some View {
        GeometryReader { geometry in
            ScrollView {
                LazyVStack(spacing: 16) {
                    ForEach(model.store.pageSlots, id: \.index) { slot in
                        PageView(slot: slot, zoom: model.store.zoom) { model.retry(page: slot.index) }
                            // `render` is a no-op when the slot is already current,
                            // so re-appearing during scroll costs nothing. A zoom
                            // change makes every visible slot stale, which is what
                            // re-renders them at the new DPI instead of upscaling.
                            .onAppear { model.render(page: slot.index) }
                            .onChange(of: model.store.zoom) { _ in model.render(page: slot.index) }
                            // `pageSlots` reuses the same `index` values across
                            // documents, so without a generation-keyed identity
                            // here `ForEach` would treat a newly opened document's
                            // rows as the *same* views as the previous document's:
                            // `onAppear` would never refire, and every page would
                            // stay stuck on its "Rendering…" placeholder forever.
                            .id("\(model.store.generation)-\(slot.index)")
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
        }
    }

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
