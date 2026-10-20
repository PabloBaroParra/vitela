import SwiftUI

/// Text, pages, size, opacity and angle in one sheet — Windows' and Linux's
/// "Add watermark" dialog, starting from the core's diagonal look. Stays open
/// on a refused choice, showing the reason under the fields.
struct WatermarkSheet: View {
    @ObservedObject var model: ViewerViewModel

    private enum PageChoice: Hashable {
        case all, current, range
    }

    @State private var text = Watermarking.defaultText
    @State private var pageChoice = PageChoice.all
    @State private var range = ""
    @State private var fontSize: Double
    @State private var opacity: Double
    @State private var angle: Double
    @State private var refusal: String?
    @State private var adding = false
    @FocusState private var textFocused: Bool

    init(model: ViewerViewModel) {
        self.model = model
        let look = model.store.client.defaultWatermarkLook()
        _fontSize = State(initialValue: look.fontSize)
        _opacity = State(initialValue: look.opacityPercent)
        _angle = State(initialValue: look.rotationDegrees)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Add watermark").font(.headline)
            TextField("Text", text: $text)
                .textFieldStyle(.roundedBorder)
                .frame(minWidth: 320)
                .focused($textFocused)
                .accessibilityIdentifier("watermark-text")
            Picker("Pages", selection: $pageChoice) {
                Text("All pages").tag(PageChoice.all)
                Text("Current page").tag(PageChoice.current)
                HStack {
                    Text("Pages")
                    TextField("1-3,7", text: $range)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 160)
                        .disabled(pageChoice != .range)
                        .accessibilityIdentifier("watermark-page-range")
                }
                .tag(PageChoice.range)
            }
            .pickerStyle(.radioGroup)
            HStack(spacing: 16) {
                lookField("Size", unit: "pt", value: $fontSize, in: Watermarking.fontSizes, step: 1)
                lookField("Opacity", unit: "%", value: $opacity, in: Watermarking.opacityPercents, step: 5)
                lookField("Angle", unit: "°", value: $angle, in: Watermarking.rotations, step: 15)
            }
            Text(Watermarking.hint)
                .font(.callout)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            if let refusal {
                Text(refusal)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("watermark-refusal")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelWatermark)
                    .keyboardShortcut(.cancelAction)
                    .disabled(adding)
                Button("Add", action: add)
                    .keyboardShortcut(.defaultAction)
                    .disabled(adding)
            }
        }
        .padding()
        .frame(width: 440)
        .onAppear { textFocused = true }
        .onChange(of: text) { _ in refusal = nil }
        .onChange(of: range) { _ in refusal = nil }
    }

    private func lookField(_ title: String, unit: String, value: Binding<Double>, in bounds: ClosedRange<Double>, step: Double) -> some View {
        Stepper(value: value, in: bounds, step: step) {
            HStack(spacing: 4) {
                Text(title)
                TextField(title, value: value, formatter: NumberFormatter())
                    .textFieldStyle(.roundedBorder)
                    .frame(width: 52)
                Text(unit).foregroundStyle(.secondary)
            }
        }
    }

    private func add() {
        let pages: ViewerStore.WatermarkPages = pageChoice == .all ? .all : pageChoice == .current ? .current : .range(range)
        refusal = nil
        adding = true
        refusal = model.addWatermark(
            pages: pages, text: text, fontSize: fontSize, opacityPercent: opacity, rotationDegrees: angle
        ) { message in
            refusal = message
            adding = false
        }
        if refusal != nil { adding = false }
    }
}
