import SwiftUI

/// The window's top row: document actions, the panel and mode toggles, and
/// zoom, drawn with the shared `assets/icons` set (`ShellIconImage`). The
/// controls Linux's `editor_toolbar.rs` shows as bare icons — save, print,
/// undo, redo, zoom, fit — are icon-only here too, with their title kept for
/// VoiceOver and the tooltip; the rest keep their title beside the icon. The
/// tools panel's selection and visibility are owned by the model.
struct ViewerToolbar: View {
    @ObservedObject var model: ViewerViewModel

    var body: some View {
        // Each cell is a related group, as in Linux's FlowBox and Windows'
        // ToolbarPanel. An adaptive grid works on our macOS 12 floor (the
        // SwiftUI Layout protocol requires macOS 13).
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 260), alignment: .leading)], alignment: .leading, spacing: 8) {
            Button(action: model.selectDocument) { Label("Open PDF", shellIcon: .files) }
            Menu {
                Button("Vitela sample", action: model.openSample)
                Button("AES-128 sample (user-aes-pass)", action: model.openAes128Sample)
                Button("RC4-128 sample (user-rc4-pass)", action: model.openRc4128Sample)
            } label: {
                Label("Open Sample", shellIcon: .sample)
            }
            HStack {
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
                // The Output group's fourth button on Linux and Windows.
                Button(action: model.requestExportImages) { Label("Export images…", shellIcon: .exportImages) }
                    .labelStyle(.iconOnly)
                    .help("Export pages as images")
                    .disabled(!model.canRequestExportImages)
            }
            HStack {
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
            }
            HStack {
                Toggle("Tools", isOn: $model.showsTools)
                    .toggleStyle(.button)
                    .help("Show or hide the tools panel")
                Toggle(isOn: $model.showsPages) { Label("Pages", shellIcon: .panelLeft) }
                    .toggleStyle(.button)
                    .help("Show or hide the list of pages (⌥⌘2)")
            }
            HStack {
                Toggle(isOn: $model.isOrganizing) { Label("Organize", shellIcon: .organize) }
                    .toggleStyle(.button)
                    .help("Rotate and rearrange pages")
                    .disabled(model.store.document == nil)
                if model.store.isSigned {
                    Text("✓ Digitally signed")
                        .foregroundStyle(.secondary)
                        .help("This document carries a digital signature.")
                        .accessibilityIdentifier("signed-indicator")
                }
            }
            HStack {
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
        }
        .padding()
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Document commands")
        .accessibilityIdentifier("editor-toolbar")
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
