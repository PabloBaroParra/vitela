import SwiftUI

/// The selected field's structure in Edit fields mode — Windows' style
/// inspector, name box and coordinate boxes, plus Delete. Text boxes commit
/// on Return or when focus leaves them; Escape restores the stored value.
struct FormFieldInspector: View {
    @ObservedObject var model: ViewerViewModel
    let field: FormField

    @State private var name = ""
    @State private var size = ""
    @State private var x = ""
    @State private var y = ""
    @State private var width = ""
    @State private var height = ""
    /// The color panel's picks, held until they settle — one restyle, one
    /// undo step, and the last pick never dropped behind work in flight.
    @StateObject private var colorSettler = ColorSettler()

    private var store: ViewerStore { model.store }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Selected field").font(.subheadline).fontWeight(.semibold)
            committingField("Name", text: $name, reset: field.name) {
                model.enqueuePageWork(store.beginFieldRename(name))
            }
            HStack {
                Picker("Font", selection: Binding(
                    get: { field.style.font },
                    set: { restyle(font: $0) }
                )) {
                    ForEach(FormFont.allCases, id: \.self) { Text($0.name).tag($0) }
                }
                // Typed sizes are brought inside 4–400 pt and the stepper
                // moves by one, as Linux's and Windows' spin boxes do.
                committingField("Size", text: $size, reset: Self.number(field.style.sizePt)) {
                    guard let value = Double(size).flatMap(FormTextStyle.clampedSize) else {
                        size = Self.number(field.style.sizePt)
                        return
                    }
                    size = Self.number(value)
                    restyle(size: value)
                }
                .frame(width: 60)
                Stepper("Size", value: Binding(
                    get: { field.style.sizePt },
                    set: { restyle(size: $0) }
                ), in: FormTextStyle.sizeRange, step: 1)
                .labelsHidden()
                .padding(.top, 14)
            }
            ColorPicker("Text color", selection: Binding(
                get: {
                    let color = colorSettler.color(for: field.id) ?? field.style.color
                    return CGColor(srgbRed: CGFloat(color.r) / 255, green: CGFloat(color.g) / 255, blue: CGFloat(color.b) / 255, alpha: 1)
                },
                set: { [model, store] in
                    colorSettler.pick(AnnotationToolbar.pdfColor($0), for: field.id) {
                        model.enqueuePageWork(store.beginFieldColor(id: $0, color: $1))
                    }
                }
            ))
            HStack {
                committingField("X (pt)", text: $x, reset: Self.number(field.rect.x), commit: commitGeometry)
                committingField("Y (pt)", text: $y, reset: Self.number(field.rect.y), commit: commitGeometry)
            }
            HStack {
                committingField("Width (pt)", text: $width, reset: Self.number(field.rect.width), commit: commitGeometry)
                committingField("Height (pt)", text: $height, reset: Self.number(field.rect.height), commit: commitGeometry)
            }
            Button("Delete Field", role: .destructive) { model.enqueuePageWork(store.beginFieldDelete()) }
                .help("Delete the selected field (⌫)")
        }
        .disabled(store.pageWorkInFlight)
        .onAppear(perform: load)
        .onChange(of: field) { _ in load() }
        // A color still settling lands on its field before another is shown,
        // or before the inspector goes away with the selection.
        .onChange(of: field.id) { _ in colorSettler.flush() }
        .onDisappear { colorSettler.flush() }
    }

    private func load() {
        name = field.name
        size = Self.number(field.style.sizePt)
        x = Self.number(field.rect.x)
        y = Self.number(field.rect.y)
        width = Self.number(field.rect.width)
        height = Self.number(field.rect.height)
    }

    private func committingField(_ label: String, text: Binding<String>, reset: String, commit: @escaping () -> Void) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(label).font(.caption).foregroundStyle(.secondary)
            TextField(label, text: text)
                .textFieldStyle(.roundedBorder)
                .labelsHidden()
                .onSubmit(commit)
                .onExitCommand { text.wrappedValue = reset }
        }
    }

    private func commitGeometry() {
        guard let x = Double(x), let y = Double(y), let width = Double(width), let height = Double(height) else {
            store.formStatus = "Enter finite field coordinates."
            return load()
        }
        model.enqueuePageWork(store.beginFieldGeometryEdit(x: x, y: y, width: width, height: height))
    }

    private func restyle(font: FormFont? = nil, size: Double? = nil, color: PdfColor? = nil) {
        var style = field.style
        if let font { style.font = font }
        if let size { style.sizePt = size }
        if let color { style.color = color }
        model.enqueuePageWork(store.beginFieldRestyle(style))
    }

    private static func number(_ value: Double) -> String {
        value == value.rounded() ? String(Int(value)) : String(format: "%.2f", value)
    }
}
