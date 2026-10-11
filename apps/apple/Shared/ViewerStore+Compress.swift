// Compress: a smaller copy of the open document, leaving the open document,
// its pending edits and its undo history exactly as they were. Mirrors
// Windows' `MainWindow.Compress.cs` and its `CompressionWording`, word for
// word: the destination is asked for last, because whether there is a file
// worth writing is only known once the compression has run.
import Foundation

extension ViewerStore {
    struct CompressRequest {
        let generation: UInt
        let document: any PdfDocument
        let preset: CompressPreset
    }

    func compressionRefusal() -> String? {
        guard let document else { return "The document is no longer open." }
        return client.compressionRefusal(document: document)
    }

    func beginCompress(_ preset: CompressPreset) -> CompressRequest? {
        guard let document else { return nil }
        return CompressRequest(generation: generation, document: document, preset: preset)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func compressionWillInvalidateSignatures(for request: CompressRequest) -> Result<Bool, EditFailure> {
        Result { try client.compressionWillInvalidateSignatures(document: request.document) }.mapError(Self.editFailure)
    }

    /// Thread-safe: only the immutable client and the request's own document.
    func compressResult(for request: CompressRequest, acknowledgingSignatureLoss: Bool) -> Result<CompressionResult, EditFailure> {
        Result {
            try client.compress(document: request.document, preset: request.preset, acknowledgingSignatureLoss: acknowledgingSignatureLoss)
        }
        .mapError(Self.editFailure)
    }
}

/// Windows' `CompressionWording`, shared by every Apple shell.
enum CompressionWording {
    static func describe(_ preset: CompressPreset) -> (name: String, description: String) {
        switch preset {
        case .lossless:
            return ("Lossless", "Repack the file only. Every page renders exactly as it does now.")
        case .balanced:
            return ("Balanced", "Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk.")
        case .small:
            return ("Small", "Also bring images down to 96 DPI. For when the file has to fit through something and the pixels matter less than that.")
        }
    }

    /// Powers of ten, the way Finder counts.
    static func humanSize(_ bytes: Int) -> String {
        let size = Double(bytes)
        if size >= 1_000_000 { return String(format: "%.1f MB", locale: Locale(identifier: "en_US_POSIX"), size / 1_000_000) }
        if size >= 1_000 { return String(format: "%.0f kB", locale: Locale(identifier: "en_US_POSIX"), size / 1_000) }
        return bytes == 1 ? "1 byte" : "\(bytes) bytes"
    }

    /// Truncated rather than rounded, so the number shown is one the file
    /// actually reached: 49.7% smaller reads 49%, never 50%.
    static func percentSmaller(_ result: CompressionResult) -> Int {
        result.beforeBytes == 0 ? 0 : result.savedBytes * 100 / result.beforeBytes
    }

    static func reductionSummary(_ result: CompressionResult) -> String {
        "\(humanSize(result.beforeBytes)) uncompressed → \(humanSize(result.afterBytes)) compressed "
            + "(\(percentSmaller(result))% smaller). Choose where to write it.\(refusalClause(result))"
    }

    /// No destination is asked for: the bytes on offer are the ones an
    /// ordinary save would write, and a panel here would invite filing a copy
    /// under a name that promises it is smaller.
    static func noGainSummary(_ result: CompressionResult) -> String {
        "Nothing to gain: this document compresses to the same \(humanSize(result.beforeBytes)) "
            + "an uncompressed save writes, so no file was written.\(refusalClause(result))"
    }

    static func writtenSummary(_ result: CompressionResult, destination: String) -> String {
        "Compressed PDF written to \(destination) (\(humanSize(result.beforeBytes)) → "
            + "\(humanSize(result.afterBytes)), \(percentSmaller(result))% smaller)."
    }

    /// The core's refusal list is open-ended; one the shell did not ask about
    /// must still reach the reader instead of vanishing.
    private static func refusalClause(_ result: CompressionResult) -> String {
        result.refusals.isEmpty ? "" : " Not everything could be done: \(result.refusals.joined(separator: "; "))."
    }
}
