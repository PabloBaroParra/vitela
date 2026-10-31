import SwiftUI

/// Signing-source presentation only. The model retains the native prompts,
/// refusal checks, key access and atomic output for each existing flow.
struct SigningPanel: View {
    @ObservedObject var model: ViewerViewModel

    private var store: ViewerStore { model.store }
    private var certificateDisabled: Bool {
        store.document == nil || model.isSaving || store.signingRefusal != nil
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Signing").font(.headline)
            Button("Draw Signature…", action: model.drawSignature)
                .help("Draw a signature, then click a page to place it")
                .disabled(store.document == nil || !store.annotationEditingAllowed)
                .accessibilityIdentifier("sign-draw")
            Text("A drawn signature is a visual mark, not a digital signature.")
                .font(.caption).foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Group {
                Button("Certificate File…", action: model.signWithCertificateFile)
                    .help("Sign with a PKCS#12 certificate (.pfx or .p12)")
                    .accessibilityIdentifier("sign-certificate")
                Button("Keychain Identity…", action: model.signWithKeychainIdentity)
                    .help("Sign with an identity in this Mac's keychains; its private key stays there")
                    .accessibilityIdentifier("sign-keychain")
                if model.offersTokenSigning {
                    Button("Card or Token…", action: model.signWithToken)
                        .help("Sign using a PKCS#11 module; its private key stays on the card or token")
                        .accessibilityIdentifier("sign-token")
                }
            }
            .disabled(certificateDisabled)
            if let refusal = store.signingRefusal {
                Text(refusal).font(.caption).foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding()
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Signing")
        .accessibilityIdentifier("signing-panel")
    }
}
