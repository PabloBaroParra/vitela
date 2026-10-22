import SwiftUI

/// The annotation tools — Windows' tool toggles plus Pointer — and, when a
/// placed annotation is selected, what applies to it. Arming a tool makes the
/// next drag on a page place it; Escape or Pointer returns to selecting text.
struct AnnotationToolbar: View {
    @ObservedObject var store: ViewerStore
    /// Opens Draw Signature, which lives on the view model.
    var drawSignature: () -> Void = {}
    /// Whether Organize is showing instead of the pages.
    var organizing = false
    /// The note being read, while its sheet is up.
    @State private var readNote: String?
    /// The exact position or size being typed, while its sheet is up.
    @State private var geometry: GeometrySheet.Request?

    var body: some View {
        HStack(spacing: 6) {
            Text("Annotate").foregroundStyle(.secondary)
            ForEach(AnnotationTool.allCases, id: \.self) { tool in
                Toggle(isOn: binding(for: tool)) { toolLabel(tool) }
                    .toggleStyle(.button)
                    .help("\(tool.name): drag on a page to place it")
            }
            Button("Signature…", action: drawSignature)
                .help("Draw a signature, then click a page to place it")
            Button { store.armAnnotationTool(nil) } label: { Label("Pointer", shellIcon: .select) }
                .keyboardShortcut(.escape, modifiers: [])
                .disabled(store.armedTool == nil)
            Divider().frame(height: 16)
            // Organize hides the pages they would scroll to, as on Windows.
            Button { store.stepAnnotation(forward: false) } label: { Label("Previous Annotation", shellIcon: .previous) }
                .help("Select and show the previous annotation (⌥⌘[)")
                .disabled(!store.canStepAnnotations || organizing)
            Button { store.stepAnnotation(forward: true) } label: { Label("Next Annotation", shellIcon: .next) }
                .help("Select and show the next annotation (⌥⌘])")
                .disabled(!store.canStepAnnotations || organizing)
            if let selected = store.selectedAnnotation {
                Divider().frame(height: 16)
                selectionControls(selected)
            }
            if store.document != nil && !store.annotationEditingAllowed {
                Text("This document does not permit annotations.").foregroundStyle(.secondary)
            }
            Spacer()
        }
        .disabled(store.document == nil || !store.annotationEditingAllowed)
        .padding(.horizontal)
        .padding(.bottom, 8)
        .sheet(item: readNoteBinding) { note in
            ReadNoteSheet(text: note.text) { readNote = nil }
        }
        .sheet(item: $geometry) { request in
            GeometrySheet(request: request) { geometry = nil }
        }
    }

    private var readNoteBinding: Binding<NoteText?> {
        Binding(get: { readNote.map(NoteText.init) }, set: { readNote = $0?.text })
    }

    /// What applies to the selected annotation — Windows' Delete, Nudge,
    /// Grow, color and Read note, each enabled only where it means something.
    @ViewBuilder private func selectionControls(_ selected: Annotation) -> some View {
        Button(action: store.deleteSelectedAnnotation) { Label("Delete", shellIcon: .delete) }
            .help("Delete the selected annotation (⌫)")
        Button("Nudge", action: store.nudgeSelectedAnnotation)
            .help("Move the selected annotation 12 points up and right")
        Button("Grow", action: store.growSelectedAnnotation)
            .help("Make the selected annotation a quarter bigger")
            .disabled(selected.rect == nil)
        Button("Position…") { geometry = .annotation(store, position: true) }
            .help("Type where the selected annotation goes, in points")
        Button("Size…") { geometry = .annotation(store, position: false) }
            .help("Type the selected annotation's width and height, in points")
            .disabled(selected.rect == nil)
        if selected.kind.supportsRestyle {
            ColorPicker("Color", selection: colorBinding(selected))
                .help("Change the selected annotation's color")
        }
        if selected.kind == .textNote {
            Button("Read Note") { readNote = selected.contents ?? "" }
        }
        if selected.kind == .freeText {
            Button("Edit Text", action: store.beginFreeTextEdit)
        }
    }

    /// The tool's name, with its drawing where the shared set has one; the
    /// note, text box and stamp tools have none and keep their word alone.
    @ViewBuilder private func toolLabel(_ tool: AnnotationTool) -> some View {
        if let icon = Self.icon(for: tool) {
            Label(tool.name, shellIcon: icon)
        } else {
            Text(tool.name)
        }
    }

    static func icon(for tool: AnnotationTool) -> ShellIcon? {
        switch tool {
        case .highlight: return .highlight
        case .underline: return .underline
        case .strikeout: return .strikeout
        case .shape: return .shape
        case .ink: return .ink
        case .note, .freeText, .stamp: return nil
        }
    }

    private func colorBinding(_ selected: Annotation) -> Binding<CGColor> {
        Binding(
            get: { Self.cgColor(selected.color ?? .annotationDefault) },
            set: { store.restyleSelectedAnnotation(Self.pdfColor($0)) }
        )
    }

    private static func cgColor(_ color: PdfColor) -> CGColor {
        CGColor(srgbRed: CGFloat(color.r) / 255, green: CGFloat(color.g) / 255, blue: CGFloat(color.b) / 255, alpha: 1)
    }

    /// The picked color in sRGB, rounded to the 8 bits a PDF color carries.
    static func pdfColor(_ color: CGColor) -> PdfColor {
        let srgb = color.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil) ?? color
        let parts = (srgb.components ?? [0, 0, 0]) + [0, 0, 0]
        func byte(_ value: CGFloat) -> UInt8 { UInt8(max(0, min(255, (value * 255).rounded()))) }
        return PdfColor(r: byte(parts[0]), g: byte(parts[1]), b: byte(parts[2]))
    }

    private func binding(for tool: AnnotationTool) -> Binding<Bool> {
        Binding(
            get: { store.armedTool == tool },
            set: { store.armAnnotationTool($0 ? tool : nil) }
        )
    }
}

/// A note's text as a sheet item.
private struct NoteText: Identifiable {
    let text: String
    var id: String { text }
}
