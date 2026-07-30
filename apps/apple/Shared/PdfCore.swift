// Shared by the macOS and iOS shells. Nothing here may import AppKit or UIKit:
// the moment it does, one of the two platforms stops compiling. Platform
// chrome (app entry, file picking, views) stays in apps/macos and apps/ios.
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
    func open(bytes: Data, password: String?) throws -> any PdfDocument
    func render(document: any PdfDocument, page: Int, dpi: Int) throws -> RenderedPage
}

enum ViewerFailure: Error, Equatable {
    /// The bytes never reached the core — the file could not be read from disk.
    case readFailed(String)
    case openFailed(String)
    /// The document is encrypted and no password (or an incomplete one) was
    /// supplied — distinct from `wrongPassword` so the prompt can tell a
    /// first ask apart from a retry.
    case passwordRequired
    /// The supplied password matched neither the user nor owner password.
    case wrongPassword
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
        case .passwordRequired: return "This document requires a password."
        case .wrongPassword: return "The password is incorrect. Try again."
        case let .renderFailed(_, message): return message
        case let .invalidImage(page): return "Page \(page + 1) returned invalid image data."
        }
    }
}
