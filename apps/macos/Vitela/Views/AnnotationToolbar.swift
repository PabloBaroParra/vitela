import SwiftUI

/// The annotation tools — Windows' Highlight / Underline / Strikeout / Shape
/// toggles plus Pointer. Arming one makes the next drag on a page place it;
/// Escape or Pointer returns to selecting text.
struct AnnotationToolbar: View {
    @ObservedObject var store: ViewerStore

    var body: some View {
        HStack(spacing: 6) {
            Text("Annotate").foregroundStyle(.secondary)
            ForEach(AnnotationTool.allCases, id: \.self) { tool in
                Toggle(tool.name, isOn: binding(for: tool))
                    .toggleStyle(.button)
                    .help("\(tool.name): drag on a page to place it")
            }
            Button("Pointer") { store.armAnnotationTool(nil) }
                .keyboardShortcut(.escape, modifiers: [])
                .disabled(store.armedTool == nil)
            if store.document != nil && !store.annotationEditingAllowed {
                Text("This document does not permit annotations.").foregroundStyle(.secondary)
            }
            Spacer()
        }
        .disabled(store.document == nil || !store.annotationEditingAllowed)
        .padding(.horizontal)
        .padding(.bottom, 8)
    }

    private func binding(for tool: AnnotationTool) -> Binding<Bool> {
        Binding(
            get: { store.armedTool == tool },
            set: { store.armAnnotationTool($0 ? tool : nil) }
        )
    }
}
