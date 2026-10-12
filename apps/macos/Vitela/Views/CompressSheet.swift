import SwiftUI

/// One choice per preset, Balanced pre-selected — the core's own default
/// offer, as in Windows' "Compress PDF" dialog. No predicted saving: nothing
/// can answer that without running.
struct CompressSheet: View {
    @ObservedObject var model: ViewerViewModel
    @State private var preset = CompressPreset.balanced

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Compress PDF").font(.headline)
            Text(sizeLine).foregroundStyle(.secondary)
            Picker("Compression", selection: $preset) {
                ForEach(CompressPreset.allCases, id: \.self) { choice in
                    let text = CompressionWording.describe(choice)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(text.name).fontWeight(.semibold)
                        Text(text.description)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }
                    .tag(choice)
                }
            }
            .pickerStyle(.radioGroup)
            .labelsHidden()
            HStack {
                Spacer()
                Button("Cancel", role: .cancel, action: model.cancelCompress)
                    .keyboardShortcut(.cancelAction)
                Button("Compress") { model.compress(preset) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 420)
    }

    private var sizeLine: String {
        guard let bytes = model.store.sourceByteCount else {
            return "No source file size is available for this new document."
        }
        return "This file is \(CompressionWording.humanSize(bytes)) on disk."
    }
}
