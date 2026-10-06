package dev.vitela.pdf.viewer

import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

internal const val SIGNATURE_NOT_REMEMBERED = "Your signature could not be remembered. Tap a page to place it."
internal const val SIGNATURE_FORGOTTEN = "Your saved signature was deleted from this phone."

/**
 * **Draw signature** (T-088): the pad, and the one signature the user may ask
 * to be remembered on this phone. With one remembered, the tile offers it
 * first — use it, draw a new one, or delete it — instead of a blank pad.
 *
 * [arm] hands a PNG to the image stamp, after releasing whatever else would
 * take the next page tap.
 */
internal class SignatureDrawing(
    private val session: ViewerSession,
    private val store: SignatureStore,
    private val arm: (ByteArray) -> Unit,
) {
    private val state = session.state

    /** Reads the remembered signature off the main thread, then offers it — or the pad when there is none. */
    fun open() {
        if (!state.value.annotationEditingAllowed) return
        val documentId = state.value.documentId
        session.scope.launch {
            val saved = withContext(session.io) { store.load() }
            val current = state.value
            // A tap that lands after another document opened, or on an open dialog, shows nothing new.
            if (current.documentId != documentId || !current.annotationEditingAllowed) return@launch
            if (current.signaturePadOpen || current.signatureChoice != null) return@launch
            state.value = if (saved != null) current.copy(signatureChoice = saved) else current.copy(signaturePadOpen = true)
        }
    }

    fun closePad() {
        state.value = state.value.copy(signaturePadOpen = false)
    }

    /**
     * Arms the image stamp with the drawn signature's [png], and keeps it on
     * the phone when [remember] — replacing any signature kept before; a
     * drawing used once leaves that one alone. [documentId] is the document
     * the pad was drawn over: the PNG is rendered off the main thread, and a
     * document opened meanwhile must not receive it — nor may a pad the user
     * cancelled while it rendered.
     */
    fun useDrawn(documentId: Long, png: ByteArray?, remember: Boolean) {
        if (documentId != state.value.documentId || !state.value.signaturePadOpen) return
        state.value = state.value.copy(signaturePadOpen = false)
        if (png == null) {
            state.value = state.value.copy(status = SIGNATURE_UNRENDERABLE)
            return
        }
        arm(png)
        if (!remember) return
        session.scope.launch {
            val saved = withContext(session.io) { store.save(png) }
            if (!saved) state.value = state.value.copy(status = SIGNATURE_NOT_REMEMBERED)
        }
    }

    /** The offer's **Use**: arms the remembered signature, with no pad. */
    fun useSaved(documentId: Long) {
        val png = state.value.signatureChoice ?: return
        if (documentId != state.value.documentId) return
        state.value = state.value.copy(signatureChoice = null)
        arm(png)
    }

    /** The offer's **Draw new**: the pad, whose drawing replaces the remembered one only if it is remembered too. */
    fun drawNew() {
        if (state.value.signatureChoice == null) return
        state.value = state.value.copy(signatureChoice = null, signaturePadOpen = true)
    }

    /** The offer's **Delete**: forgets the signature on this phone; nothing is armed. */
    fun deleteSaved() {
        if (state.value.signatureChoice == null) return
        state.value = state.value.copy(signatureChoice = null, status = SIGNATURE_FORGOTTEN)
        session.scope.launch { withContext(session.io) { store.delete() } }
    }

    fun closeChoice() {
        if (state.value.signatureChoice != null) state.value = state.value.copy(signatureChoice = null)
    }
}
