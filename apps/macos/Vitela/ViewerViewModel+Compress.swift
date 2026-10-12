import Foundation

/// File ▸ Compress…: the macOS chain around `ViewerStore+Compress.swift`.
/// Mirrors Windows' `MainWindow.Compress.cs`: the core's refusal before the
/// dialog, a preset, the compression's own signature question, the
/// compression itself off the main thread — and only when something smaller
/// came out, a destination, written atomically. Nothing is reopened: the
/// open document keeps its edits and history.
extension ViewerViewModel {
    func requestCompress() {
        guard store.document != nil, !isSaving else { return }
        if let refusal = store.compressionRefusal() {
            store.editStatus = refusal
            return
        }
        isCompressPresented = true
    }

    func cancelCompress() {
        isCompressPresented = false
        store.editStatus = "Compression cancelled."
    }

    func compress(_ preset: CompressPreset) {
        isCompressPresented = false
        guard !isSaving, let request = store.beginCompress(preset) else { return }
        isSaving = true
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let invalidates = self.store.compressionWillInvalidateSignatures(for: request)
            DispatchQueue.main.async {
                switch invalidates {
                case let .failure(failure):
                    self.finishCompress(failure.localizedDescription)
                case .success(true) where !self.prompts.confirmSignatureLoss():
                    self.finishCompress("Compression cancelled.")
                case let .success(invalidates):
                    self.runCompression(request, acknowledgingSignatureLoss: invalidates)
                }
            }
        }
    }

    private func runCompression(_ request: ViewerStore.CompressRequest, acknowledgingSignatureLoss: Bool) {
        store.editStatus = "Compressing PDF…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let outcome = self.store.compressResult(for: request, acknowledgingSignatureLoss: acknowledgingSignatureLoss)
            DispatchQueue.main.async {
                switch outcome {
                case let .failure(failure):
                    self.finishCompress(failure.localizedDescription)
                case let .success(result) where !result.reduced:
                    self.finishCompress(CompressionWording.noGainSummary(result))
                case let .success(result):
                    self.store.editStatus = CompressionWording.reductionSummary(result)
                    self.writeCompressed(result)
                }
            }
        }
    }

    private func writeCompressed(_ result: CompressionResult) {
        // The same neutral name Save's siblings suggest on Linux and Windows.
        guard let destination = prompts.chooseDestination("document.pdf") else {
            return finishCompress("Compression cancelled. No file was written.")
        }
        store.editStatus = "Writing the compressed PDF…"
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let written = Result { try result.bytes.write(to: destination, options: .atomic) }
            DispatchQueue.main.async {
                switch written {
                case .success:
                    self.finishCompress(CompressionWording.writtenSummary(result, destination: destination.path))
                case let .failure(error):
                    self.finishCompress("Could not write the file: \(error.localizedDescription)")
                }
            }
        }
    }

    private func finishCompress(_ status: String) {
        isSaving = false
        store.editStatus = status
    }
}
