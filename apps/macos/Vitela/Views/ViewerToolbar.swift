import SwiftUI

/// The window's top row: document actions, the panel and mode toggles, and
/// zoom. Moved out of `ViewerRootView` unchanged; the panels it toggles are
/// still the root view's state, handed in as bindings.
struct ViewerToolbar: View {
    @ObservedObject var model: ViewerViewModel
    @Binding var showsProperties: Bool
    @Binding var showsComments: Bool
    @Binding var showsForm: Bool
    /// The selected image's typed position or size, while its sheet is up.
    @State private var imageGeometry: GeometrySheet.Request?

    var body: some View {
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
            Button("Share…", action: model.share)
                .help("Send a copy with your changes through Mail, Messages, AirDrop…")
                .disabled(!model.canShare)
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
            Toggle("Pages", isOn: $model.showsPages)
                .toggleStyle(.button)
                .help("Show or hide the list of pages (⌥⌘2)")
            Toggle("Comments", isOn: $showsComments)
                .toggleStyle(.button)
                .help("The document's comments, by page")
            Toggle("Form", isOn: $showsForm)
                .toggleStyle(.button)
                .help("Fill in the document's form fields")
            Toggle("Edit Content", isOn: Binding(get: { model.store.contentEditMode }, set: model.setContentEditMode))
                .toggleStyle(.button)
                .help("Click the page's own text to retype, move or delete it, or an image to move, resize or delete it")
                .disabled(!model.canEditContent && !model.store.contentEditMode)
            Toggle("Add Text", isOn: Binding(get: { model.textInsertArmed }, set: model.armTextInsert))
                .toggleStyle(.button)
                .help("Click a page to add a new line of text there")
                .disabled(!model.canEditContent)
            // Armed, the status line asks for the click; choosing again replaces the image.
            Button("Add Image…", action: model.armImageInsert)
                .help("Choose a PNG or JPEG, then click a page to place it")
                .disabled(!model.canEditContent)
            if model.store.selectedContentImage != nil {
                Button("Replace Image…", action: model.replaceSelectedImage)
                    .help("Swap the selected image's picture for a PNG or JPEG, keeping its box")
                    .disabled(model.store.pageWorkInFlight)
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
        .sheet(item: $imageGeometry) { request in
            GeometrySheet(request: request) { imageGeometry = nil }
        }
    }

    /// On while `mode` is active; switching it off freezes the factor the
    /// pages are shown at, as an explicit zoom.
    private func zoomModeBinding(_ mode: ZoomMode) -> Binding<Bool> {
        Binding(
            get: { model.store.zoomMode == mode },
            set: { model.store.setZoomMode($0 ? mode : .custom) }
        )
    }
}
