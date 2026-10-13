import CryptoKit
import Foundation
import Security

/// The signing identities in the user's keychains — macOS's counterpart of
/// Windows' `WindowsSigningStore` (the current user's personal store). Keys
/// stay in the Keychain or on the smart card behind it: only public
/// certificates and signatures over digests cross to the core, and macOS
/// asks the user before a key is used, as it does for any app.
final class KeychainSigningStore: NativeSigningStore {
    private struct Entry {
        let identity: NativeSigningIdentity
        let key: SecKey
    }

    private let entries: [Entry]

    /// Every identity with an RSA or P-256 key, in Keychain order; anything
    /// else cannot sign here and is left out, as on Windows.
    init(identities: [SecIdentity]) {
        var seen = Set<String>()
        entries = identities.compactMap { identity in
            guard let entry = Self.entry(for: identity), seen.insert(entry.identity.id).inserted else { return nil }
            return entry
        }
    }

    /// The user's keychains, searched for identities. No identities is not an
    /// error: the caller says so in its own words.
    static func open() throws -> KeychainSigningStore {
        let query: [CFString: Any] = [
            kSecClass: kSecClassIdentity,
            kSecMatchLimit: kSecMatchLimitAll,
            kSecReturnRef: true,
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        switch status {
        case errSecSuccess:
            return KeychainSigningStore(identities: (result as? [SecIdentity]) ?? [])
        case errSecItemNotFound:
            return KeychainSigningStore(identities: [])
        default:
            throw KeychainSigningFailure(message: "The Keychain could not be read (\(Self.describe(status))).")
        }
    }

    func identities() throws -> [NativeSigningIdentity] {
        entries.map(\.identity)
    }

    func signDigest(identityId: String, digest: Data, algorithm: NativeSigningAlgorithm) throws -> Data {
        guard digest.count == 32, let entry = entries.first(where: { $0.identity.id == identityId }) else {
            throw KeychainSigningFailure(message: "The signing identity is no longer available.")
        }
        let secAlgorithm: SecKeyAlgorithm = algorithm == .rsaSha256
            ? .rsaSignatureDigestPKCS1v15SHA256
            // X9.62 is the ASN.1 DER `SEQUENCE { r, s }` the core expects.
            : .ecdsaSignatureDigestX962SHA256
        var error: Unmanaged<CFError>?
        guard algorithm == entry.identity.algorithm,
              let signature = SecKeyCreateSignature(entry.key, secAlgorithm, digest as CFData, &error)
        else {
            _ = error?.takeRetainedValue()
            throw KeychainSigningFailure(message: "macOS could not use this signing key, or authentication was cancelled.")
        }
        return signature as Data
    }

    /// What a key of these attributes can sign with — Windows' OID switch,
    /// limited to the P-256 curve the core signs with.
    static func algorithm(keyType: String?, keySizeInBits: Int?) -> NativeSigningAlgorithm? {
        if keyType == kSecAttrKeyTypeRSA as String { return .rsaSha256 }
        if keyType == kSecAttrKeyTypeECSECPrimeRandom as String, keySizeInBits == 256 { return .ecdsaSha256 }
        return nil
    }

    private static func entry(for identity: SecIdentity) -> Entry? {
        var certificate: SecCertificate?
        var key: SecKey?
        guard SecIdentityCopyCertificate(identity, &certificate) == errSecSuccess, let certificate,
              SecIdentityCopyPrivateKey(identity, &key) == errSecSuccess, let key,
              let attributes = SecKeyCopyAttributes(key) as? [CFString: Any],
              let algorithm = algorithm(
                  keyType: attributes[kSecAttrKeyType] as? String,
                  keySizeInBits: (attributes[kSecAttrKeySizeInBits] as? NSNumber)?.intValue
              )
        else { return nil }
        let leaf = SecCertificateCopyData(certificate) as Data
        let id = SHA256.hash(data: leaf).map { String(format: "%02X", $0) }.joined()
        let name = (SecCertificateCopySubjectSummary(certificate) as String?) ?? "Unnamed certificate"
        return Entry(
            identity: NativeSigningIdentity(id: id, displayName: name, certificateChainDer: chain(of: certificate), algorithm: algorithm),
            key: key
        )
    }

    /// The leaf and whatever issuers the keychains hold, leaf first. Built
    /// offline only to embed the public chain — discovery is not trust
    /// validation, so the evaluation's verdict is ignored, as on Windows.
    private static func chain(of certificate: SecCertificate) -> [Data] {
        let leaf = SecCertificateCopyData(certificate) as Data
        var trust: SecTrust?
        guard SecTrustCreateWithCertificates(certificate, SecPolicyCreateBasicX509(), &trust) == errSecSuccess, let trust else {
            return [leaf]
        }
        SecTrustSetNetworkFetchAllowed(trust, false)
        _ = SecTrustEvaluateWithError(trust, nil)
        guard let chain = SecTrustCopyCertificateChain(trust) as? [SecCertificate], !chain.isEmpty else { return [leaf] }
        return [leaf] + chain.dropFirst().map { SecCertificateCopyData($0) as Data }
    }

    private static func describe(_ status: OSStatus) -> String {
        (SecCopyErrorMessageString(status, nil) as String?) ?? "error \(status)"
    }
}

/// A Keychain refusal, already a sentence for the reader.
struct KeychainSigningFailure: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}
