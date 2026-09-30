package dev.vitela.pdf.core

/** One identity a certificate file can sign as: [id] goes back to [PdfDocument.sign], [name] is what the user picks by. */
data class SigningIdentity(val id: String, val name: String)

/**
 * An unlocked `.pfx`/`.p12` file. It holds private keys in native memory, so
 * whoever unlocked it closes it as soon as the signing it was unlocked for
 * is over, signed or not.
 */
interface SigningCertificate : AutoCloseable {
    /** Every identity in the file that can sign; empty for a file holding only certificates. */
    val identities: List<SigningIdentity>
}
