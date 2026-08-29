// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// The only Swift source that calls the generated UniFFI API. `pdf_ffi.swift`
/// is generated into each shell's `Generated` directory before Xcode compiles
/// the target.
final class UniFfiPdfCoreClient: PdfCoreClient {
    /// `pdf-render` resolves `PDFIUM_DYNAMIC_LIB_PATH` lazily, on the first
    /// successful bind, and caches it from then on — so this must run before
    /// any FFI call, which construction order guarantees.
    init(bundle: Bundle = .main) {
        setenv("PDFIUM_DYNAMIC_LIB_PATH", Self.pdfiumPath(in: bundle), 1)
    }

    /// The one genuine platform difference in the shared core. A macOS app
    /// bundle nests its payload under `Contents/`; an iOS bundle is flat, so
    /// the same `Frameworks` directory sits directly beside the executable.
    /// Both shells' build phases copy `libpdfium.dylib` into that directory.
    private static func pdfiumPath(in bundle: Bundle) -> String {
        #if os(macOS)
        let frameworks = bundle.bundleURL.appendingPathComponent("Contents/Frameworks", isDirectory: true)
        #else
        let frameworks = bundle.bundleURL.appendingPathComponent("Frameworks", isDirectory: true)
        #endif
        return frameworks.appendingPathComponent("libpdfium.dylib").path
    }

    func open(bytes: Data, password: String?) throws -> any PdfDocument {
        do {
            let handle = try openFromBytes(bytes: bytes, password: password)
            let pages = handle.pageDimensions().map {
                PageDimensions(width: $0.widthPt, height: $0.heightPt)
            }
            return UniFfiDocument(handle: handle, pages: pages)
        } catch FfiError.PasswordRequired {
            throw ViewerFailure.passwordRequired
        } catch FfiError.WrongPassword {
            throw ViewerFailure.wrongPassword
        } catch {
            // `String(describing:)`, not `localizedDescription`: the generated
            // errors are plain Swift enums, whose localized description is the
            // useless "operation couldn't be completed" boilerplate.
            throw ViewerFailure.openFailed(String(describing: error))
        }
    }

    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage {
        guard let document = document as? UniFfiDocument else {
            throw ViewerFailure.renderFailed(page: page, message: "document is not backed by UniFFI")
        }
        do {
            let bitmap = try renderPage(
                handle: document.handle,
                pageIndex: UInt32(page),
                dpi: UInt32(dpi),
                options: FfiRenderOptions(invertContentColors: false)
            )
            return try RenderedPage(
                rgba: bitmap.getPixels(),
                width: Int(bitmap.width()),
                height: Int(bitmap.height()),
                stride: Int(bitmap.stride())
            )
        } catch {
            throw ViewerFailure.renderFailed(page: page, message: String(describing: error))
        }
    }

    func search(document: any PdfDocument, query: String) throws -> [SearchMatch] {
        guard let document = document as? UniFfiDocument else {
            throw TextQueryFailure.failed("document is not backed by UniFFI")
        }
        do {
            return try document.handle.search(query: query).map { found in
                SearchMatch(
                    pageIndex: Int(found.pageIndex),
                    text: found.text,
                    characterBounds: found.characterBounds.map(TextRect.init)
                )
            }
        } catch FfiError.UnsupportedOperation {
            throw TextQueryFailure.notPermitted
        } catch {
            throw TextQueryFailure.failed(String(describing: error))
        }
    }

    func pageCharacters(document: any PdfDocument, page: Int) throws -> any PageCharacters {
        guard let document = document as? UniFfiDocument else {
            throw TextQueryFailure.failed("document is not backed by UniFFI")
        }
        do {
            let handle = try document.handle.pageCharacters(pageIndex: UInt32(page))
            return UniFfiPageCharacters(handle: handle)
        } catch FfiError.UnsupportedOperation {
            throw TextQueryFailure.notPermitted
        } catch {
            throw TextQueryFailure.failed(String(describing: error))
        }
    }
}

private struct UniFfiDocument: PdfDocument {
    let handle: DocumentHandle
    let pages: [PageDimensions]
}

private extension TextRect {
    init(_ rect: FfiTextRect) {
        self.init(xPt: rect.xPt, yPt: rect.yPt, widthPt: rect.widthPt, heightPt: rect.heightPt)
    }
}

/// Wraps the generated `FfiPageCharacters` object so `ViewerStore` only ever
/// sees the shared `PageCharacters` protocol, never a UniFFI type.
private final class UniFfiPageCharacters: PageCharacters {
    private let handle: FfiPageCharacters

    init(handle: FfiPageCharacters) {
        self.handle = handle
    }

    func caretAt(xPt: Double, yPt: Double) -> Int? {
        handle.caretAt(xPt: Float(xPt), yPt: Float(yPt)).map(Int.init)
    }

    func textIn(anchor: Int, focus: Int) -> String {
        handle.textIn(anchor: UInt32(anchor), focus: UInt32(focus))
    }

    func rectsIn(anchor: Int, focus: Int) -> [TextRect] {
        handle.rectsIn(anchor: UInt32(anchor), focus: UInt32(focus)).map(TextRect.init)
    }
}
