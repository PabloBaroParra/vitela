import SwiftUI

/// Text or picture, pages, size, opacity, angle and layer in one sheet —
/// Linux's "Add watermark" dialog (Windows' asks text only), starting from
/// the core's look for the kind chosen. Stays open on a refused choice,
/// showing the reason under the fields.
struct WatermarkSheet: View {
    @ObservedObject var model: ViewerViewModel

    private enum PageChoice: Hashable {
        case all, current, range
    }

    @State private var kind = WatermarkKind.text
    @State private var text = Watermarking.defaultText
    @State private var picture: ViewerViewModel.WatermarkPicture?
    @State private var pageChoice = PageChoice.all
    @State private var range = ""
    @State private var fontSize: Double
    @State private var scale: Double
    @State private var opacity: Double
    @State private var angle: Double
    @State private var layer: WatermarkLayer
    @State private var refusal: String?
    @State private var adding = false
    @FocusState private var textFocused: Bool

    init(model: ViewerViewModel) {
        self.model = model
        let look = model.store.client.defaultWatermarkLook(.text)
        _fontSize = State(initialValue: look.size)
        _scale = State(initialValue: model.store.client.defaultWatermarkLook(.picture).size)
        _opacity = State(initialValue: look.opacityPercent)
        _angle = State(initialValue: look.rotationDegrees)
        _layer = State(initialValue: look.layer)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Add watermark").font(.headline)
            Picker("Stamp", selection: $kind) {
                Text("Text").tag(WatermarkKind.text)
                Text("Picture").tag(WatermarkKind.picture)
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            switch kind {
            case .text:
                HStack(spacing: 16) {
                    TextField("Text", text: $text)
                        .textFieldStyle(.roundedBorder)
                        .frame(minWidth: 200)
                        .focused($textFocused)
                        .accessibilityIdentifier("watermark-text")
                    lookField("Size", unit: "pt", value: $fontSize, in: Watermarking.fontSizes, step: 1)
                }
            case .picture:
                HStack(spacing: 16) {
                    Button("Choose Picture…", action: choosePicture)
                    Text(picture?.name ?? "No picture chosen")
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .accessibilityIdentifier("watermark-picture")
                    lookField("Size", unit: "% of page", value: $scale, in: Watermarking.scalePercents, step: 5)
                }
            }
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
                lookField("Opacity", unit: "%", value: $opacity, in: Watermarking.opacityPercents, step: 5)
                lookField("Angle", unit: "°", value: $angle, in: Watermarking.rotations, step: 15)
            }
            Picker("Position", selection: $layer) {
                Text("Over the page content").tag(WatermarkLayer.overContent)
                Text("Behind the page content").tag(WatermarkLayer.underContent)
            }
            .pickerStyle(.radioGroup)
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
        .frame(width: 480)
        .onAppear { textFocused = true }
        .onChange(of: kind, perform: switchKind)
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

    /// Angle and layer go back to the new kind's look: a logo left at text's
    /// 45° reads as a mistake, and a background picture painted over the
    /// text hides the page — Linux's reasoning.
    private func switchKind(_ kind: WatermarkKind) {
        let look = model.store.client.defaultWatermarkLook(kind)
        angle = look.rotationDegrees
        layer = look.layer
        refusal = nil
    }

    private func choosePicture() {
        let (chosen, problem) = model.chooseWatermarkPicture()
        if let problem { refusal = problem }
        guard let chosen else { return }
        picture = chosen
        refusal = nil
    }

    private func add() {
        let pages: ViewerStore.WatermarkPages = pageChoice == .all ? .all : pageChoice == .current ? .current : .range(range)
        let mark: ViewerStore.WatermarkMarkAnswer = kind == .text
            ? .text(text, fontSize: fontSize)
            : .picture(picture?.bytes, scalePercent: scale)
        refusal = nil
        adding = true
        refusal = model.addWatermark(
            pages: pages, mark: mark, opacityPercent: opacity, rotationDegrees: angle, layer: layer
        ) { message in
            refusal = message
            adding = false
        }
        if refusal != nil { adding = false }
    }
}
