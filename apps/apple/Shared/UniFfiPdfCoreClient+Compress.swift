// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Compressing through the generated UniFFI API — the calls Windows' facade
/// makes (`PdfDocumentFacade.cs` `CompressAsync`), with its refusal wording.
extension UniFfiPdfCoreClient {
    func compressionRefusal(document: any PdfDocument) -> String? {
        guard let handle = try? handle(of: document) else { return "The document is no longer open." }
        return Vitela.compressionRefusal(handle: handle, intent: .default).map { EditFailure.sentence(Self.clause($0)) }
    }

    func compressionWillInvalidateSignatures(document: any PdfDocument) throws -> Bool {
        let handle = try handle(of: document)
        return try Self.mapEditErrors { try compressedSaveWillInvalidateSignatures(handle: handle, intent: .default) }
    }

    func compress(document: any PdfDocument, preset: CompressPreset, acknowledgingSignatureLoss: Bool) throws -> CompressionResult {
        let handle = try handle(of: document)
        let saved = try Self.mapEditErrors {
            try saveCompressedToBytes(
                handle: handle,
                intent: .default,
                signatures: acknowledgingSignatureLoss ? .proceedAndInvalidate : .unacknowledged,
                preset: FfiCompressPreset(preset)
            )
        }
        let report = saved.report
        return CompressionResult(
            bytes: saved.bytes,
            beforeBytes: Int(report.beforeBytes),
            afterBytes: Int(report.afterBytes),
            savedBytes: Int(report.savedBytes),
            reduced: report.outcome == .reduced,
            refusals: report.refusals.map(Self.clause)
        )
    }

    /// Windows' `RefusalSentence`, as a lower-case clause.
    private static func clause(_ refusal: FfiCompressRefusal) -> String {
        switch refusal {
        case .encryptedDocumentNotRewritable:
            return "this document's password protection does not allow it to be rewritten, so it cannot be compressed"
        case .signaturesWouldBeInvalidated:
            return "compressing rewrites the file and would stop its signature from verifying"
        case let .other(detail):
            return detail
        }
    }
}

private extension FfiCompressPreset {
    init(_ preset: CompressPreset) {
        switch preset {
        case .lossless: self = .lossless
        case .balanced: self = .balanced
        case .small: self = .small
        }
    }
}
