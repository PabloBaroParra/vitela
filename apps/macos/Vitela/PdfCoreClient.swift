import Foundation

struct PageDimensions: Equatable {
    let width: Double
    let height: Double
}

struct RenderedPage: Equatable {
    let rgba: Data
    let width: Int
    let height: Int
    let stride: Int

    static let placeholder = RenderedPage(rgba: Data([0, 0, 0, 0]), width: 1, height: 1, stride: 4)
}

protocol PdfDocument {
    var pages: [PageDimensions] { get }
}

/// Renders are issued from a background queue, so a conforming client must be
/// safe to call from any thread. `UniFfiPdfCoreClient` is: it holds no mutable
/// state and every handle it hands out is `Arc`/`Mutex`-guarded on the Rust side.
protocol PdfCoreClient {
    func open(bytes: Data) throws -> any PdfDocument
    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage
}

enum ViewerFailure: Error, Equatable {
    /// The bytes never reached the core — the file could not be read from disk.
    case readFailed(String)
    case openFailed(String)
    case renderFailed(page: Int, message: String)
    case invalidImage(page: Int)
}

extension ViewerFailure: LocalizedError {
    var errorDescription: String? { message }

    /// Deliberately not named `localizedDescription`: that would shadow the
    /// `Error` extension and silently change meaning at each call site.
    var message: String {
        switch self {
        case let .readFailed(message): return message
        case let .openFailed(message): return message
        case let .renderFailed(_, message): return message
        case let .invalidImage(page): return "Page \(page + 1) returned invalid image data."
        }
    }
}

/// The only Swift source that calls the generated UniFFI API. `pdf_ffi.swift`
/// is generated into `apps/macos/Generated` before Xcode compiles this target.
final class UniFfiPdfCoreClient: PdfCoreClient {
    /// `pdf-render` resolves `PDFIUM_DYNAMIC_LIB_PATH` lazily, on the first
    /// successful bind, and caches it from then on — so this must run before
    /// any FFI call, which construction order guarantees.
    init(bundle: Bundle = .main) {
        let frameworks = bundle.bundleURL.appendingPathComponent("Contents/Frameworks", isDirectory: true)
        setenv("PDFIUM_DYNAMIC_LIB_PATH", frameworks.appendingPathComponent("libpdfium.dylib").path, 1)
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
