// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
//
// The failures `PdfCoreClient` reports, in words a reader can act on.
import Foundation

/// `search`/`pageCharacters` read the document's text, so they answer to the
/// same extraction permission a copy does.
enum TextQueryFailure: Error, Equatable {
    /// The document forbids text extraction.
    case notPermitted
    case failed(String)
}

extension TextQueryFailure: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .notPermitted: return "Text extraction is not permitted for this document."
        case let .failed(message): return message
        }
    }
}

/// An edit or save the core refused, in words a user can act on.
enum EditFailure: Error, Equatable {
    /// The document's permissions forbid this kind of change.
    case notPermitted(String)
    /// The save would break a signature and the user has not agreed to that.
    case signaturesWouldBeInvalidated
    case failed(String)
}

extension EditFailure: LocalizedError {
    var errorDescription: String? {
        switch self {
        case let .notPermitted(message): return message
        case .signaturesWouldBeInvalidated: return "Saving would break this document's signature."
        case let .failed(message): return message
        }
    }
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
