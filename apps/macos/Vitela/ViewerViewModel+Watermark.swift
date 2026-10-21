import Foundation

/// Add Watermark… on macOS: the sheet, its picture picker, and keeping it
/// open until the core has answered. The rules are the store's
/// (`ViewerStore+Watermark.swift`).
///
/// The sheet stays up while the stamp is recorded, so a refusal only the
/// core can give — a character Helvetica cannot show, a file that is not a
/// picture — lands under the fields, where the reader corrects it, as on
/// Windows and Linux.
extension ViewerViewModel {
    func requestWatermark() {
        guard canEditContent else { return }
        guard store.editingAllowed else {
            store.editStatus = ContentEditing.forbidden
            return
        }
        isWatermarkPresented = true
    }

    /// The sheet's Add. Returns why it must stay open, or `nil` once the
    /// stamp is on its way; `refused` hears the core's refusal if it comes.
    func addWatermark(
        pages: ViewerStore.WatermarkPages, mark: ViewerStore.WatermarkMarkAnswer, opacityPercent: Double, rotationDegrees: Double,
        layer: WatermarkLayer, refused: @escaping (String) -> Void
    ) -> String? {
        let (problem, request) = store.beginWatermark(
            pages: pages, mark: mark, opacityPercent: opacityPercent, rotationDegrees: rotationDegrees, layer: layer
        )
        if let problem { return problem }
        guard let request else { return "Another change is still being applied. Try again in a moment." }
        enqueuePageWork(request) { [weak self] outcome in
            // Recorded is recorded: a refresh that failed after the stamp
            // still closes the sheet, the step is there to undo.
            if !outcome.applied, case let .failure(failure) = outcome.pages {
                refused(failure.localizedDescription)
            } else {
                self?.isWatermarkPresented = false
            }
        }
        return nil
    }

    /// A picture Choose Picture… read: its bytes, untouched, and the file's
    /// name to show beside the button.
    struct WatermarkPicture: Equatable {
        let bytes: Data
        let name: String
    }

    /// Choose Picture…: the picked PNG or JPEG read once, or what to say
    /// instead; neither when the picker was cancelled.
    func chooseWatermarkPicture() -> (picture: WatermarkPicture?, problem: String?) {
        guard let url = prompts.chooseImage() else { return (nil, nil) }
        guard let bytes = try? Data(contentsOf: url) else { return (nil, "The selected image could not be read.") }
        return (WatermarkPicture(bytes: bytes, name: url.lastPathComponent), nil)
    }

    func cancelWatermark() {
        isWatermarkPresented = false
        store.editStatus = Watermarking.cancelled
    }
}
