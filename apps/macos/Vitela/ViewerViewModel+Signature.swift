import Foundation

/// Draw Signature: offer the remembered signature first, else the pad; turn
/// the strokes into a PNG off the main thread; remember it when asked; arm
/// the Stamp tool with it so the next click places it — Windows'
/// `MainWindow.DrawnSignature.cs`.
extension ViewerViewModel {
    /// The sheet the flow is showing.
    enum SignatureSheet: Identifiable, Equatable {
        case saved(Data)
        case pad
        var id: String {
            if case .saved = self { return "saved" }
            return "pad"
        }
    }

    func drawSignature() {
        guard store.document != nil, store.annotationEditingAllowed else { return }
        let generation = store.generation
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let saved = self.signatures.load()
            DispatchQueue.main.async {
                guard self.store.generation == generation else { return }
                self.signatureSheet = saved.map(SignatureSheet.saved) ?? .pad
            }
        }
    }

    func chooseSavedSignature(_ choice: SavedSignatureChoice) {
        guard case let .saved(png) = signatureSheet else { return }
        signatureSheet = nil
        switch choice {
        case .use:
            store.armDrawnSignature(png)
        case .drawNew:
            // After this sheet is gone, so the pad is not stacked on it.
            DispatchQueue.main.async { [weak self] in self?.signatureSheet = .pad }
        case .delete:
            operationQueue.addOperation { [weak self] in self?.signatures.delete() }
            store.editStatus = "Your saved signature was deleted from this Mac."
        case .cancel:
            break
        }
    }

    func useDrawnSignature(_ strokes: [[PadPoint]], remember: Bool) {
        signatureSheet = nil
        let generation = store.generation
        operationQueue.addOperation { [weak self] in
            guard let self else { return }
            let png = SignaturePng.render(strokes)
            let kept = png.map { remember ? self.signatures.save($0) : true } ?? true
            DispatchQueue.main.async {
                // A document opened while the image was made must not get it.
                guard self.store.generation == generation else { return }
                guard let png else {
                    self.store.editStatus = "The signature could not be turned into an image."
                    return
                }
                self.store.armDrawnSignature(png)
                if !kept {
                    self.store.editStatus = "Your signature could not be remembered. Click a page to place it."
                }
            }
        }
    }
}
