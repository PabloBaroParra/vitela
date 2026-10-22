import SwiftUI

/// Home's "Start a new task" target — Linux's `home/hero.rs` drop zone and
/// Windows' `HomeDropZone`: a dashed area that opens a PDF dropped on it, or
/// the file chooser when clicked anywhere, lit in the accent colour while a
/// drag is over it. The whole window already takes drops; this is the place
/// that says so. What a dropped file becomes is `openExternalFiles`' call,
/// the same as anywhere else in the window.
struct HomeDropZone: View {
    @ObservedObject var model: ViewerViewModel
    @State private var targeted = false

    private let shape = RoundedRectangle(cornerRadius: 14)

    var body: some View {
        VStack(spacing: 10) {
            Text("Drag and drop a PDF here").font(.title3)
            Text("or click anywhere in this area to open one")
                .font(.callout)
                .foregroundStyle(.secondary)
            Button("Select file", action: model.selectDocument)
                .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity)
        .padding(28)
        .background(shape.fill(targeted ? Color.accentColor.opacity(0.12) : Color(nsColor: .textBackgroundColor)))
        // Dashed at rest, solid accent while a drag is over it — the
        // `.home-dropzone` / `.drop-active` pair.
        .overlay(shape.strokeBorder(
            targeted ? Color.accentColor : Color(nsColor: .separatorColor),
            style: StrokeStyle(lineWidth: 2, dash: targeted ? [] : [4, 3])
        ))
        .contentShape(shape)
        // The button keeps its own click, so one click never raises two
        // choosers.
        .onTapGesture(perform: model.selectDocument)
        .onDrop(of: [.fileURL], isTargeted: $targeted) { providers in
            ViewerRootView.loadFileURLs(providers, then: model.openExternalFiles)
            return !providers.isEmpty
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Drop a PDF here to open it")
    }
}
