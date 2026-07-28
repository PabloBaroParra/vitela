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

    func open(bytes: Data) throws -> any PdfDocument {
        do {
            let handle = try openFromBytes(bytes: bytes, password: nil)
            let pages = handle.pageDimensions().map {
                PageDimensions(width: $0.widthPt, height: $0.heightPt)
            }
            return UniFfiDocument(handle: handle, pages: pages)
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
}

private struct UniFfiDocument: PdfDocument {
    let handle: DocumentHandle
    let pages: [PageDimensions]
}
