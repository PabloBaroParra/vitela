import SwiftUI

/// Pages, format and resolution in one sheet — Windows' "Export pages as
/// images" dialog. Stays open on a refused choice, showing the reason under
/// the fields, so the reader corrects it in place.
struct ExportImagesSheet: View {
    @ObservedObject var model: ViewerViewModel

    private enum PageChoice: Hashable {
        case all, current, range
    }

    @State private var pageChoice = PageChoice.all
    @State private var range = ""
    @State private var format = ImageFormat.png
    @State private var dpi = ViewerStore.defaultExportDpi
    @State private var refusal: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Export pages as images").font(.headline)
            Picker("Pages", selection: $pageChoice) {
                Text("All pages").tag(PageChoice.all)
                Text("Current page").tag(PageChoice.current)
                HStack {
                    Text("Pages")
                    TextField("1-3,7", text: $range)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 160)
                        .disabled(pageChoice != .range)
                        .accessibilityIdentifier("export-page-range")
                }
                .tag(PageChoice.range)
            }
            .pickerStyle(.radioGroup)
            Picker("Format", selection: $format) {
                ForEach(ImageFormat.allCases, id: \.self) { Text($0.name).tag($0) }
            }
            .pickerStyle(.segmented)
            .frame(width: 220)
            Stepper(value: $dpi, in: ViewerStore.exportDpiRange, step: 1) {
                HStack {
                    Text("Resolution")
                    TextField("DPI", value: $dpi, formatter: NumberFormatter())
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 60)
                        .accessibilityIdentifier("export-dpi")
                    Text("DPI")
                }
            }
            if let refusal {
                Text(refusal)
                    .foregroundColor(.red)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("export-refusal")
            }
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelExportImages)
                    .keyboardShortcut(.cancelAction)
                Button("Export", action: export)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 380)
    }

    private func export() {
        refusal = model.exportImages(
            pages: pageChoice == .all ? .all : pageChoice == .current ? .current : .range(range), dpi: dpi, format: format
        )
    }
}
