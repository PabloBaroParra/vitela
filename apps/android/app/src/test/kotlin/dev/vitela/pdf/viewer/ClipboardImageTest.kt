package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * T-091: only a clipboard image backed by local content is read. Text — a
 * copied URL above all — never is, so a paste cannot become a download.
 */
class ClipboardImageTest {
    @Test
    fun anImageBehindAContentUriIsRead() {
        assertEquals("content://media/1", pastableImageUri(listOf("image/png"), listOf("content://media/1")))
    }

    @Test
    fun copiedTextIsNeverRead() {
        assertNull(pastableImageUri(listOf("text/plain"), listOf(null)))
        assertNull(pastableImageUri(listOf("text/uri-list"), listOf("https://example.com/a.png")))
    }

    @Test
    fun anImageTypeOverARemoteUriIsNotFetched() {
        assertNull(pastableImageUri(listOf("image/png"), listOf("https://example.com/a.png")))
        assertNull(pastableImageUri(listOf("image/png"), listOf("file:///sdcard/a.png")))
    }

    @Test
    fun theFirstContentUriWinsAndTheSchemeIgnoresCase() {
        assertEquals("CONTENT://b", pastableImageUri(listOf("image/jpeg"), listOf<String?>(null, "http://a", "CONTENT://b", "content://c")))
    }

    @Test
    fun anEmptyClipboardOffersNothing() {
        assertNull(pastableImageUri(emptyList(), emptyList()))
    }
}
