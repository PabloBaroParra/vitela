import SwiftUI
import UniformTypeIdentifiers

struct ViewerRootView: View {
    @ObservedObject var model: ViewerViewModel

    private static let zoomStep = 0.1

    var body: some View {
        NavigationView {
            content
                .navigationBarTitleDisplayMode(.inline)
                .navigationTitle(model.title)
                .toolbar {
                    ToolbarItem(placement: .navigationBarLeading) {
                        Button("Open PDF", action: model.selectDocument)
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
        .accessibilityIdentifier("viewer-root")
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
                }
            }
            .padding()
        }
    }
}
