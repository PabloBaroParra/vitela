import SwiftUI

/// The pad — Windows' "Draw your signature" dialog. Use stays disabled until
/// a line exists; Clear wipes the pad without closing; "Remember on this Mac"
/// is ticked by default.
struct SignaturePadSheet: View {
    let use: (_ strokes: [[PadPoint]], _ remember: Bool) -> Void
    let cancel: () -> Void

    @State private var strokes: [[PadPoint]] = []
    @State private var remember = true
    /// The next drag sample starts a stroke rather than extending the last.
    @State private var isNewStroke = true

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Draw your signature").font(.headline)
            Canvas { context, _ in
                for stroke in strokes where !stroke.isEmpty {
                    var path = Path()
                    path.move(to: CGPoint(x: stroke[0].x, y: stroke[0].y))
                    for point in stroke.dropFirst() { path.addLine(to: CGPoint(x: point.x, y: point.y)) }
                    context.stroke(path, with: .color(.black), style: StrokeStyle(lineWidth: DrawnSignature.strokeWidth, lineCap: .round, lineJoin: .round))
                }
            }
            .frame(width: 360, height: 160)
            .background(Color.white)
            .border(Color.secondary.opacity(0.5))
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { value in
                        let point = PadPoint(x: Double(value.location.x), y: Double(value.location.y))
                        if isNewStroke {
                            strokes.append([point])
                            isNewStroke = false
                        } else {
                            strokes[strokes.count - 1].append(point)
                        }
                    }
                    .onEnded { _ in isNewStroke = true }
            )
            .accessibilityLabel("Signature pad")
            Toggle("Remember on this Mac", isOn: $remember)
            HStack {
                Button("Clear") { strokes = [] }
                    .disabled(strokes.isEmpty)
                Spacer()
                Button("Cancel", role: .cancel, action: cancel)
                    .keyboardShortcut(.cancelAction)
                Button("Use") { use(strokes, remember) }
                    .disabled(!DrawnSignature.hasInk(strokes))
            }
        }
        .padding()
    }
}

/// The signature remembered on this Mac, offered before a blank pad: Use
/// places it, Draw New opens the pad, Delete forgets it — Windows'
/// "Your signature" dialog. Shown on white, as the pad is.
struct SavedSignatureSheet: View {
    let png: Data
    let choose: (SavedSignatureChoice) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Your signature").font(.headline)
            Group {
                if let image = NSImage(data: png) {
                    Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
                } else {
                    Text("This saved signature can no longer be shown.").foregroundColor(.black)
                }
            }
            .frame(width: 360, height: 160)
            .background(Color.white)
            .border(Color.secondary.opacity(0.5))
            .accessibilityLabel("Your saved signature")
            HStack {
                Button("Delete", role: .destructive) { choose(.delete) }
                Spacer()
                Button("Cancel", role: .cancel) { choose(.cancel) }
                    .keyboardShortcut(.cancelAction)
                Button("Draw New") { choose(.drawNew) }
                Button("Use") { choose(.use) }
                    .keyboardShortcut(.defaultAction)
                    .disabled(NSImage(data: png) == nil)
            }
        }
        .padding()
    }
}

enum SavedSignatureChoice {
    case use, drawNew, delete, cancel
}
