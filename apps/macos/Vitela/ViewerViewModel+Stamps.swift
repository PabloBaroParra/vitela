import Foundation

/// The image half of placing a stamp: once a Stamp drag has marked where it
/// goes (`ViewerStore.pendingStamp`), ask for the picture, read it off the
/// main thread, and hand the bytes to the store — Windows'
/// `PlaceImageStampAsync`.
extension ViewerViewModel {
    func resolvePendingStamp() {
        guard let pending = store.pendingStamp else { return }
        guard let url = prompts.chooseImage() else { return store.cancelPendingStamp() }
        let generation = store.generation
        operationQueue.addOperation { [weak self] in
            let bytes = try? Data(contentsOf: url)
            DispatchQueue.main.async {
                guard let self else { return }
                // Neither an old page position nor old bytes may reach a
                // document opened while the picker was up.
                guard self.store.generation == generation, self.store.pendingStamp == pending else { return }
                guard let bytes else {
                    self.store.pendingStamp = nil
                    self.store.editStatus = "The selected stamp image could not be read."
                    return
                }
                self.store.insertPendingStamp(imageBytes: bytes)
            }
        }
    }
}
