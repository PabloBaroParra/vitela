package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.SigningIdentity
import dev.vitela.pdf.text.UiText

/**
 * The open Sign dialog, one step at a time: why the document cannot be signed
 * ([refusal]), else the certificate file picked ([certificateName], null until
 * one is), the identities it unlocked to ([identities], empty until its
 * password is accepted), the one to sign as, and the last [error]. The
 * certificate's password and keys are not here: the password stays in the
 * dialog's field and the keys in [Signing], so the published state never
 * holds a secret.
 */
data class SignEditor(
    val refusal: UiText? = null,
    val certificateName: String? = null,
    val identities: List<SigningIdentity> = emptyList(),
    val selectedIdentityId: String? = null,
    val error: UiText? = null,
    /** True while the password is being checked, so a second tap cannot unlock twice. */
    val unlocking: Boolean = false,
)

// The first sentence is the Linux shell's (`app::sign`), so the same refusal
// reads the same on both.
internal val SAVE_BEFORE_SIGNING: UiText = UiText.Resource(R.string.error_sign_unsaved)
internal val CERTIFICATE_LOCKED: UiText = UiText.Resource(R.string.error_sign_certificate_locked)
internal val CERTIFICATE_CANNOT_SIGN: UiText = UiText.Resource(R.string.error_sign_no_identity)

/** What the certificate picker offers: PKCS#12 by its MIME types, and anything, since many providers label a .pfx as a plain binary. */
internal val CERTIFICATE_MIME_TYPES = arrayOf("application/x-pkcs12", "application/pkcs12", "application/octet-stream", "*/*")

internal fun signedFileName(title: String): String = "${documentStem(title)}-signed.pdf"
