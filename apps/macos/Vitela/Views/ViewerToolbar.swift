import SwiftUI

/// The window's top row: document actions, the panel and mode toggles, and
/// zoom. Moved out of `ViewerRootView` unchanged; the panels it toggles are
/// still the root view's state, handed in as bindings.
struct ViewerToolbar: View {
    @ObservedObject var model: ViewerViewModel
    @Binding var showsProperties: Bool
    @Binding var showsComments: Bool
    @Binding var showsForm: Bool

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
            Toggle("Edit Text", isOn: Binding(get: { model.store.contentEditMode }, set: model.setContentEditMode))
                .toggleStyle(.button)
                .help("Click a line of the page's own text to retype or delete it")
                .disabled(!model.canEditContent && !model.store.contentEditMode)
            Toggle("Add Text", isOn: Binding(get: { model.textInsertArmed }, set: model.armTextInsert))
                .toggleStyle(.button)
                .help("Click a page to add a new line of text there")
                .disabled(!model.canEditContent)
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
}
