// Editing the open document: the properties (Info dictionary) panel,
// undo/redo, and saving. Mirrors the Windows facade's metadata and save
// paths (`PdfDocumentFacade.Metadata.cs`, `MainWindow.Save.cs`): every edit
// lands in the core's in-memory model and stays undoable; only a save turns
// it into bytes.
import Foundation

extension ViewerStore {
    enum DocumentProperty: CaseIterable {
        case title, author, subject, keywords, creator, producer
    }

    enum DocumentDateProperty: CaseIterable {
        case created, modified
    }

    /// A save in flight. Like `RenderRequest`, it carries everything the
    /// background queue needs, so `saveResult(for:)` never reads the store.
    struct SaveRequest {
        let generation: UInt
        let revision: Int
        let document: any PdfDocument
        let acknowledgingSignatureLoss: Bool
    }

    var hasUnsavedChanges: Bool {
        document != nil && editRevision != savedRevision
    }

    /// Called by `open` once a document has replaced the previous one.
    func resetEditing() {
        editRevision = 0
        savedRevision = 0
        canUndo = false
        canRedo = false
        editStatus = ""
        guard let document else {
            documentInfo = nil
            editingAllowed = false
            return
        }
        editingAllowed = client.contentEditingAllowed(document: document)
        documentInfo = try? client.documentInfo(document: document)
    }

    // MARK: - Properties

    /// Sets one text property; an empty string removes it from the file.
    /// Does nothing when the value is unchanged, so retyping the same text
    /// never adds an undo step.
    func setDocumentProperty(_ property: DocumentProperty, to text: String) {
        mutateDocumentInfo { info in
            let value: String? = text.isEmpty ? nil : text
            switch property {
            case .title: info.title = value
            case .author: info.author = value
            case .subject: info.subject = value
            case .keywords: info.keywords = value
            case .creator: info.creator = value
            case .producer: info.producer = value
            }
        }
    }

    /// Parses `text` as a metadata date and sets it. Invalid text leaves the
    /// document untouched, reports why and returns `false`; the caller shows
    /// the stored value again (`documentInfo` still holds it).
    @discardableResult
    func setDocumentDate(_ property: DocumentDateProperty, to text: String) -> Bool {
        guard let current = documentInfo else { return false }
        let prior = property == .created ? current.creationDate : current.modDate
        let parsed: MetadataDate?
        do {
            parsed = try MetadataDateText.parse(text, offset: prior?.offset ?? .utc)
        } catch {
            editStatus = "Invalid date, reverted: \(error.localizedDescription)"
            return false
        }
        return mutateDocumentInfo { info in
            switch property {
            case .created: info.creationDate = parsed
            case .modified: info.modDate = parsed
            }
        }
    }

    @discardableResult
    private func mutateDocumentInfo(_ mutate: (inout DocumentInfo) -> Void) -> Bool {
        guard let document, let before = documentInfo else { return false }
        guard editingAllowed else {
            editStatus = "This document does not permit metadata changes."
            return false
        }
        var after = before
        mutate(&after)
        guard after != before else { return true }
        do {
            try client.setDocumentInfo(document: document, info: after)
        } catch {
            editStatus = Self.editMessage(for: error)
            return false
        }
        documentInfo = after
        recordEdit(status: "Document properties updated. Changes are pending save.")
        return true
    }

    // MARK: - Undo / redo

    func undo() {
        guard let document, client.undo(document: document) else { return }
        recordEdit(status: "Undone.")
        refreshAfterHistoryStep(document)
    }

    func redo() {
        guard let document, client.redo(document: document) else { return }
        recordEdit(status: "Redone.")
        refreshAfterHistoryStep(document)
    }

    /// An undo or redo can touch any part of the model; re-read what the
    /// panels show rather than guess which step it was.
    private func refreshAfterHistoryStep(_ document: any PdfDocument) {
        documentInfo = try? client.documentInfo(document: document)
    }

    private func recordEdit(status: String) {
        editRevision += 1
        editStatus = status
        // The core's EditLog is the single owner of history; asking it back
        // is simpler than mirroring its rules (a new edit drops the redo tail).
        guard let document else { return }
        canUndo = client.canUndo(document: document)
        canRedo = client.canRedo(document: document)
    }

    // MARK: - Save

    func beginSave(acknowledgingSignatureLoss: Bool) -> SaveRequest? {
        guard let document else { return nil }
        return SaveRequest(
            generation: generation,
            revision: editRevision,
            document: document,
            acknowledgingSignatureLoss: acknowledgingSignatureLoss
        )
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func saveWillInvalidateSignatures(for request: SaveRequest) -> Result<Bool, EditFailure> {
        Result { try client.saveWillInvalidateSignatures(document: request.document) }
            .mapError(Self.editFailure)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func saveResult(for request: SaveRequest) -> Result<Data, EditFailure> {
        Result {
            try client.save(document: request.document, acknowledgingSignatureLoss: request.acknowledgingSignatureLoss)
        }
        .mapError(Self.editFailure)
    }

    /// Records that the bytes for `request` reached disk. Edits made while
    /// the save ran stay unsaved: `savedRevision` is the revision the bytes
    /// were computed from, not the current one.
    func applySaved(_ request: SaveRequest) {
        guard request.generation == generation else { return }
        savedRevision = request.revision
        editStatus = "PDF saved. Changes remain editable in this session."
    }

    func reportSaveFailure(_ message: String) {
        editStatus = message
    }

    static func editFailure(_ error: Error) -> EditFailure {
        (error as? EditFailure) ?? .failed(String(describing: error))
    }

    static func editMessage(for error: Error) -> String {
        editFailure(error).localizedDescription
    }
}
