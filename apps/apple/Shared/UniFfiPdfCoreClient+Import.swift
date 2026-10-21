// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// Importing other PDFs' pages through the generated UniFFI API — the same
/// prepare-then-apply calls the Linux (`organize/import.rs`) and Windows
/// (`MainWindow.OrganizeImport.cs`) shells make.
extension UniFfiPdfCoreClient {
    func importRefusal(document: any PdfDocument) -> String? {
        guard let handle = try? handle(of: document) else { return "The document is no longer open." }
        return Vitela.importRefusal(handle: handle).map(EditFailure.sentence)
    }

    func prepareImport(bytes: Data, password: String?) throws -> any ImportSource {
        do {
            return UniFfiImportSource(prepared: try Vitela.prepareImport(bytes: bytes, password: password))
        } catch FfiError.PasswordRequired {
            throw ViewerFailure.passwordRequired
        } catch FfiError.WrongPassword {
            throw ViewerFailure.wrongPassword
        } catch let error as FfiError {
            throw EditFailure(error)
        }
    }

    func importPrepared(document: any PdfDocument, sources: [any ImportSource]) throws -> ImportedBatch {
        let handle = try handle(of: document)
        let prepared = try sources.map { source -> PreparedImport in
            guard let source = source as? UniFfiImportSource else {
                throw EditFailure.failed("import source is not backed by UniFFI")
            }
            return source.prepared
        }
        let report = try Self.mapEditErrors {
            try Vitela.importPrepared(handle: handle, sources: prepared, index: UInt32(handle.pageCount()))
        }
        return ImportedBatch(pageCount: Int(report.pageCount), sourceIds: report.sourceIds)
    }

    func documentBlocks(document: any PdfDocument) -> [DocumentBlock] {
        guard let handle = try? handle(of: document) else { return [] }
        return Vitela.documentBlocks(handle: handle).map { block in
            let source: BlockSource
            switch block.source {
            case .base: source = .base
            case .blank: source = .blank
            case let .imported(id): source = .imported(id: id)
            }
            return DocumentBlock(source: source, part: block.part.map(Int.init), start: Int(block.start), count: Int(block.count))
        }
    }
}

/// Wraps the generated `PreparedImport` so the store only ever sees the
/// shared `ImportSource` protocol.
private struct UniFfiImportSource: ImportSource {
    let prepared: PreparedImport

    var pageCount: Int { Int(prepared.pageCount()) }
    var warnings: [String] { prepared.warnings() }
}
