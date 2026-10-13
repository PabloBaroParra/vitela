// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// What print and export render from. Mirrors Windows'
/// `PdfDocumentFacade.OutputSnapshot.cs` (`SnapshotIfAnnotated`) and Linux's
/// `write/snapshot.rs`.
///
/// Neither may render an annotated session from its own document. That
/// document draws the preview, and the preview leaves this session's
/// annotations out on purpose: `AnnotationOverlay` draws them, so baking them
/// in would paint each one twice. Print and export have no overlay pass, so a
/// page rendered from it silently loses every annotation the reader added.
/// The core's output snapshot is the document as a save would write it,
/// reopened under the session's own password.
struct OutputSource {
    let document: any PdfDocument
    /// Whether the session holds annotations the preview leaves out.
    let annotated: Bool
}

extension ViewerStore {
    /// Main thread: what the next print or export starts from, or `nil` with
    /// no document open.
    func beginOutput() -> OutputSource? {
        guard let document else { return nil }
        return OutputSource(document: document, annotated: !annotations.isEmpty)
    }

    /// Thread-safe, like `renderResult(for:)`: touches only the immutable
    /// client. The document itself when nothing is missing from it — no
    /// save that could fail — or else a snapshot, which is a whole save in
    /// memory, so call it off the main thread where the caller can.
    func outputDocument(for source: OutputSource) -> Result<any PdfDocument, EditFailure> {
        guard source.annotated else { return .success(source.document) }
        do {
            return .success(try client.outputSnapshot(document: source.document))
        } catch let failure as EditFailure {
            return .failure(failure)
        } catch {
            return .failure(.failed(error.localizedDescription))
        }
    }
}
