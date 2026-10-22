import SwiftUI

/// The window's top row: document actions, the panel and mode toggles, and
/// zoom, drawn with the shared `assets/icons` set (`ShellIconImage`). The
/// controls Linux's `editor_toolbar.rs` shows as bare icons — save, print,
/// undo, redo, zoom, fit — are icon-only here too, with their title kept for
/// VoiceOver and the tooltip; the rest keep their title beside the icon. The
/// panels it toggles are the root view's state, handed in as bindings.
struct ViewerToolbar: View {
    @ObservedObject var model: ViewerViewModel
    @Binding var showsProperties: Bool
    @Binding var showsComments: Bool
    @Binding var showsForm: Bool
    /// The selected image's typed position or size, while its sheet is up.
    @State private var imageGeometry: GeometrySheet.Request?

    var body: some View {
        HStack {
            Toggle(isOn: Binding(get: { model.showsHome }, set: { $0 ? model.showHome() : model.returnToDocument() })) {
                Label("Home", shellIcon: .home)
            }
                .toggleStyle(.button)
                .help("Show the start screen; the open document stays as it is (⇧⌘H)")
                .disabled(model.store.document == nil)
            Button(action: model.selectDocument) { Label("Open PDF", shellIcon: .files) }
            Menu {
                Button("Vitela sample", action: model.openSample)
                Button("AES-128 sample (user-aes-pass)", action: model.openAes128Sample)
                Button("RC4-128 sample (user-rc4-pass)", action: model.openRc4128Sample)
            } label: {
                Label("Open Sample", shellIcon: .sample)
            }
            Button { model.saveAs() } label: { Label("Save As…", shellIcon: .save) }
                .labelStyle(.iconOnly)
                .help("Save as (⌘S)")
                .disabled(model.store.document == nil || model.isSaving)
            Button(action: model.printDocument) { Label("Print…", shellIcon: .print) }
                .labelStyle(.iconOnly)
                .help("Print (⌘P)")
                .disabled(model.store.document == nil)
            Button(action: model.share) { Label("Share…", shellIcon: .share) }
                .help("Send a copy with your changes through Mail, Messages, AirDrop…")
                .disabled(!model.canShare)
            Button("Copy", action: model.copySelection)
                .keyboardShortcut("c", modifiers: .command)
                .disabled(model.store.selectedText == nil)
            Button(action: model.undo) { Label("Undo", shellIcon: .undo) }
                .labelStyle(.iconOnly)
                .help("Undo (⌘Z)")
                .disabled(!model.store.canUndo || model.store.pageWorkInFlight)
            Button(action: model.redo) { Label("Redo", shellIcon: .redo) }
                .labelStyle(.iconOnly)
                .help("Redo (⇧⌘Z)")
                .disabled(!model.store.canRedo || model.store.pageWorkInFlight)
            Toggle("Properties", isOn: $showsProperties)
                .toggleStyle(.button)
            if model.store.isSigned {
                Text("✓ Digitally signed")
                    .foregroundStyle(.secondary)
                    .help("This document carries a digital signature.")
                    .accessibilityIdentifier("signed-indicator")
            }
            Toggle(isOn: $model.showsPages) { Label("Pages", shellIcon: .panelLeft) }
                .toggleStyle(.button)
                .help("Show or hide the list of pages (⌥⌘2)")
            Toggle(isOn: $showsComments) { Label("Comments", shellIcon: .comments) }
                .toggleStyle(.button)
                .help("The document's comments, by page")
            Toggle("Form", isOn: $showsForm)
                .toggleStyle(.button)
                .help("Fill in the document's form fields")
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
            // Armed, the status line asks for the click; choosing again replaces the image.
            Button(action: model.armImageInsert) { Label("Add Image…", shellIcon: .image) }
                .help("Choose a PNG or JPEG, then click a page to place it")
                .disabled(!model.canEditContent)
            Button("Add Watermark…", action: model.requestWatermark)
                .help("Stamp the same text or picture across the pages you pick, as one undo step")
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
            Toggle(isOn: $model.isOrganizing) { Label("Organize", shellIcon: .organize) }
                .toggleStyle(.button)
                .help("Rotate and rearrange pages")
                .disabled(model.store.document == nil)
            Spacer()
            Button { model.store.stepZoom(-1) } label: { Label("Zoom out", shellIcon: .zoomOut) }
                .labelStyle(.iconOnly)
                .help("Zoom out (⌘−)")
            Text("\(Int((model.store.displayedZoom * 100).rounded()))%")
                // A monospaced face keeps the readout from jittering as the
                // digit count changes.
                .font(.system(.body, design: .monospaced))
            Button { model.store.stepZoom(1) } label: { Label("Zoom in", shellIcon: .zoomIn) }
                .labelStyle(.iconOnly)
                .help("Zoom in (⌘=)")
            Toggle(isOn: zoomModeBinding(.fitWidth)) { Label("Fit width", shellIcon: .fitWidth) }
                .toggleStyle(.button)
                .labelStyle(.iconOnly)
                .help("Fit each page to the window width")
            Toggle(isOn: zoomModeBinding(.fitPage)) { Label("Fit page", shellIcon: .fitPage) }
                .toggleStyle(.button)
                .labelStyle(.iconOnly)
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
