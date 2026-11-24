package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.text.UiText

/**
 * The open Protect dialog: why the document cannot be protected ([refusal], a
 * sentence) or null when it can, whether the protected copy breaks the file's
 * signature — in which case confirming is the user's yes to that — and the
 * last password [error]. The passwords themselves are not here: they stay in
 * the dialog's own fields, so the published state never holds a secret.
 */
data class ProtectEditor(
    val refusal: UiText? = null,
    val signaturesWillBreak: Boolean = false,
    val error: UiText? = null,
)

// Every sentence below is the Windows shell's (MainWindow.Protect.cs), so one
// protection reads the same on every platform.

internal val PROTECTION_REFUSED: UiText = UiText.Resource(R.string.error_protect_forbidden)

/**
 * What is wrong with the two passwords, or null when the core will take them.
 * The core refuses both cases too; asking here keeps the dialog open with the
 * reason instead of discovering it after a destination was chosen.
 */
internal fun protectionPasswordProblem(openPassword: String, permissionsPassword: String): UiText? = when {
    openPassword.isEmpty() -> UiText.Resource(R.string.error_protect_open_missing)
    permissionsPassword.isEmpty() -> UiText.Resource(R.string.error_protect_permissions_missing)
    openPassword == permissionsPassword -> UiText.Resource(R.string.error_protect_same_passwords)
    else -> null
}

internal fun protectedFileName(title: String): String = "${documentStem(title)}-protected.pdf"
