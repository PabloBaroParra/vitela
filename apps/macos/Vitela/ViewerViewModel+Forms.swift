import Foundation

/// Filling a form field: the store decides whether there is anything to do
/// (`ViewerStore+Forms.swift`); the core work runs on the queue like any
/// page work, since the value is painted by the PDF.
extension ViewerViewModel {
    func fillField(_ fieldId: UInt64, value: FieldValue) {
        enqueuePageWork(store.beginFieldFill(fieldId, value: value))
    }
}
