import SwiftUI

/// Existing content-edit commands, moved from the document toolbar unchanged.
/// The model/store still own mode, selection, permission and queued page work.
struct ContentEditPanel: View {
    @ObservedObject var model: ViewerViewModel
    @State private var imageGeometry: GeometrySheet.Request?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Edit PDF").font(.headline)
            Toggle(isOn: Binding(get: { model.store.contentEditMode }, set: model.setContentEditMode)) {
                Label("Edit Content", shellIcon: .edit)
            }
            .toggleStyle(.button)
            .help("Click the page's own text to retype, move or delete it, or an image to move, resize or delete it")
            .disabled(!model.canEditContent && !model.store.contentEditMode)
            Toggle(isOn: Binding(get: { model.textInsertArmed }, set: model.armTextInsert)) {
                Label("Add Text", shellIcon: .text)
            }
            .toggleStyle(.button)
            .help("Click a page to add a new line of text there")
            .disabled(!model.canEditContent)
            Button(action: model.armImageInsert) { Label("Add Image…", shellIcon: .image) }
                .help("Choose a PNG or JPEG, then click a page to place it")
                .disabled(!model.canEditContent)
            Button("Add Watermark…", action: model.requestWatermark)
                .help("Stamp the same text or picture across the pages you pick, as one undo step")
                .disabled(!model.canEditContent)
            if model.store.selectedContentImage != nil {
                Divider()
                Button("Replace Image…", action: model.replaceSelectedImage)
                    .help(model.store.canReplaceSelectedImage
                        ? "Swap the selected image's picture for a PNG or JPEG, keeping its box"
                        : "This image cannot be replaced now — the status line says why")
                    .disabled(model.store.pageWorkInFlight || !model.store.canReplaceSelectedImage)
                Button("Delete Image", action: model.deleteSelectedImage)
                    .help("Delete the selected image from the page (⌫)")
                    .disabled(model.store.pageWorkInFlight)
                Button("Image Position…") { imageGeometry = model.selectedImageGeometry(position: true) }
                    .help("Type where the selected image goes, in points")
                    .disabled(model.store.pageWorkInFlight)
                Button("Image Size…") { imageGeometry = model.selectedImageGeometry(position: false) }
                    .help("Type the selected image's width and height, in points")
                    .disabled(model.store.pageWorkInFlight)
            }
        }
        .padding()
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityIdentifier("content-edit-panel")
        .sheet(item: $imageGeometry) { request in
            GeometrySheet(request: request) { imageGeometry = nil }
        }
    }
}
