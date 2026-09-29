package dev.vitela.pdf.viewer

/**
 * The open Protect dialog: why the document cannot be protected ([refusal], a
 * sentence) or null when it can, whether the protected copy breaks the file's
 * signature — in which case confirming is the user's yes to that — and the
 * last password [error]. The passwords themselves are not here: they stay in
 * the dialog's own fields, so the published state never holds a secret.
 */
data class ProtectEditor(
    val refusal: String? = null,
    val signaturesWillBreak: Boolean = false,
    val error: String? = null,
)

// Every sentence below is the Windows shell's (MainWindow.Protect.cs), so one
// protection reads the same on every platform.

internal const val PROTECTION_REFUSED = "This document does not permit protection changes."

/**
 * What is wrong with the two passwords, or null when the core will take them.
 * The core refuses both cases too; asking here keeps the dialog open with the
 * reason instead of discovering it after a destination was chosen.
 */
internal fun protectionPasswordProblem(openPassword: String, permissionsPassword: String): String? = when {
    openPassword.isEmpty() -> "Enter the password the document will ask for when it is opened."
    permissionsPassword.isEmpty() -> "Enter the permissions password."
    openPassword == permissionsPassword -> "The two passwords must be different."
    else -> null
}

internal fun protectedFileName(title: String): String = "${documentStem(title)}-protected.pdf"
