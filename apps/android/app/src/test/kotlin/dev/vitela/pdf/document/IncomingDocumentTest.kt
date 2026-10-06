package dev.vitela.pdf.document

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * T-092: a PDF handed over by another app ("Open with", the share sheet) is
 * opened only when it is local content the resolver reads — never a URL the
 * app would have to fetch, never a raw file path.
 */
class IncomingDocumentTest {
    @Test
    fun openWithHandsTheDocumentAsData() {
        assertEquals("content://docs/1", openablePdfUri(ACTION_VIEW, data = "content://docs/1", stream = null))
    }

    @Test
    fun theShareSheetHandsTheDocumentAsAStream() {
        assertEquals("content://mail/2", openablePdfUri(ACTION_SEND, data = null, stream = "content://mail/2"))
    }

    @Test
    fun eachActionReadsOnlyItsOwnSlot() {
        assertNull(openablePdfUri(ACTION_VIEW, data = null, stream = "content://docs/1"))
        assertNull(openablePdfUri(ACTION_SEND, data = "content://docs/1", stream = null))
    }

    @Test
    fun aRemoteOrFileUriIsRefused() {
        assertNull(openablePdfUri(ACTION_VIEW, data = "https://example.com/a.pdf", stream = null))
        assertNull(openablePdfUri(ACTION_VIEW, data = "file:///sdcard/a.pdf", stream = null))
        assertNull(openablePdfUri(ACTION_SEND, data = null, stream = "http://example.com/a.pdf"))
    }

    @Test
    fun theSchemeIgnoresCase() {
        assertEquals("CONTENT://docs/1", openablePdfUri(ACTION_VIEW, data = "CONTENT://docs/1", stream = null))
    }

    @Test
    fun aLauncherStartOpensNothing() {
        assertNull(openablePdfUri("android.intent.action.MAIN", data = null, stream = null))
        assertNull(openablePdfUri(null, data = "content://docs/1", stream = null))
    }

    private companion object {
        const val ACTION_VIEW = "android.intent.action.VIEW"
        const val ACTION_SEND = "android.intent.action.SEND"
    }
}
