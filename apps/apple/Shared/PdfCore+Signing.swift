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

/// What a native key can do — mirrors `FfiSigningAlgorithm`: SHA-256 digests,
/// ECDSA signatures as ASN.1 DER.
enum NativeSigningAlgorithm: Equatable {
    case rsaSha256
    case ecdsaSha256
}

/// One identity a native store offers — mirrors `FfiPlatformSigningIdentity`.
/// Only public material: the certificates, leaf first.
struct NativeSigningIdentity: Equatable {
    let id: String
    let displayName: String
    let certificateChainDer: [Data]
    let algorithm: NativeSigningAlgorithm
}

/// A platform certificate store whose private keys never leave it (the
/// macOS Keychain, Windows' personal store): the core asks it for identities
/// and for signatures over digests — the shell side of the core's
/// `PlatformSigningSource`. Implemented per platform, outside this folder,
/// because the native APIs are not Foundation.
protocol NativeSigningStore: AnyObject {
    func identities() throws -> [NativeSigningIdentity]
    /// The signature over a 32-byte SHA-256 `digest`; throws a sentence for
    /// the reader when the key cannot be used or the user cancels.
    func signDigest(identityId: String, digest: Data, algorithm: NativeSigningAlgorithm) throws -> Data
}
