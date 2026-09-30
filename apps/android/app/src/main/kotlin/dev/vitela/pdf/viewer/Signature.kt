package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.SigningIdentity

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
    val refusal: String? = null,
    val certificateName: String? = null,
    val identities: List<SigningIdentity> = emptyList(),
    val selectedIdentityId: String? = null,
    val error: String? = null,
    /** True while the password is being checked, so a second tap cannot unlock twice. */
    val unlocking: Boolean = false,
)

// The first sentence is the Linux shell's (`app::sign`), so the same refusal
// reads the same on both.
internal const val SAVE_BEFORE_SIGNING = "Save your changes before signing this document."
internal const val CERTIFICATE_LOCKED = "The certificate could not be opened. Check its password, and that the file is a .pfx or .p12 certificate."
internal const val CERTIFICATE_CANNOT_SIGN = "This certificate file holds no identity that can sign."

/** What the certificate picker offers: PKCS#12 by its MIME types, and anything, since many providers label a .pfx as a plain binary. */
internal val CERTIFICATE_MIME_TYPES = arrayOf("application/x-pkcs12", "application/pkcs12", "application/octet-stream", "*/*")

internal fun signedFileName(title: String): String = "${documentStem(title)}-signed.pdf"

/** The core's lower-case refusal clause as the sentence the dialog shows. */
internal fun signRefusalSentence(clause: String): String = "${clause.replaceFirstChar { it.uppercaseChar() }}."
