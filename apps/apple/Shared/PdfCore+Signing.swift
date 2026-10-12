// Shared by the macOS and iOS shells — see PdfCore.swift for the rule about
// not importing AppKit or UIKit here.
import Foundation

/// One identity a signing source can sign as: a certificate with its key —
/// mirrors `FfiSigningIdentity`.
struct SigningIdentity: Equatable {
    let id: String
    let displayName: String
}

/// An unlocked signing source (a `.pfx`/`.p12` file, a token, a store) —
/// mirrors the generated `SigningCertificate`: opaque, the shell only reads
/// which identities it holds and hands it back to sign.
protocol SigningSource {
    var identities: [SigningIdentity] { get }
}

/// A source that could not be unlocked with the password given. PKCS#12
/// cannot tell a wrong password from a damaged file, so the wording covers
/// both — Windows' message.
enum SigningSourceFailure: Error, Equatable {
    case wrongPassword
    case failed(String)
}

extension SigningSourceFailure: LocalizedError {
    var errorDescription: String? {
        switch self {
        case .wrongPassword: return "The password is incorrect, or the file is not a valid PKCS#12 certificate."
        case let .failed(message): return message
        }
    }
}
