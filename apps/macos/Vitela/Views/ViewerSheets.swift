import SwiftUI

/// Every sheet the viewer window presents, and how closing each one is read
/// — kept apart from `ViewerRootView`'s layout so adding a dialog does not
/// grow the window's view.
struct ViewerSheets: ViewModifier {
    @ObservedObject var model: ViewerViewModel

    func body(content: Content) -> some View {
        content
            .sheet(isPresented: $model.isGoToPagePresented) {
                GoToPageSheet(model: model)
            }
            .sheet(isPresented: $model.isExtractPresented) {
                PageRangeSheet.extract(model)
            }
            .sheet(isPresented: $model.isSplitPresented) {
                PageRangeSheet.split(model)
            }
            .sheet(isPresented: $model.isExportImagesPresented) {
                ExportImagesSheet(model: model)
            }
            .sheet(isPresented: $model.isCompressPresented) {
                CompressSheet(model: model)
            }
            .sheet(isPresented: $model.isProtectPresented) {
                ProtectSheet(model: model)
            }
            .sheet(isPresented: isPresentingNoteSheet) {
                NoteSheet(store: model.store)
            }
            .sheet(item: pendingFreeTextBinding) { pending in
                FreeTextSheet(store: model.store, pending: pending)
            }
    }

    /// Up while a traced note waits for its text; dismissing it any other
    /// way than Add is a cancel.
    private var isPresentingNoteSheet: Binding<Bool> {
        Binding(
            get: { model.store.pendingNote != nil },
            set: { if !$0 { model.store.cancelPendingNote() } }
        )
    }

    /// Up while a text box waits for its text; closing it any other way than
    /// Add/Save is a cancel.
    private var pendingFreeTextBinding: Binding<PendingFreeText?> {
        Binding(
            get: { model.store.pendingFreeText },
            set: { if $0 == nil { model.store.cancelPendingFreeText() } }
        )
    }
}
