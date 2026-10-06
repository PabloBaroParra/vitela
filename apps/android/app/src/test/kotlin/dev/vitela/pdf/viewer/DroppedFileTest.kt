package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * T-092: a file dragged in from the other half of a split screen means what
 * it means on the desktop — a PDF opens, an image becomes a stamp. Only a
 * `content:` URI qualifies, the same rule as paste and "Open with".
 */
class DroppedFileTest {
    private val types = mapOf(
        "content://docs/a.pdf" to "application/pdf",
        "content://media/b.png" to "image/png",
        "content://docs/c.txt" to "text/plain",
    )

    @Test
    fun aPdfOrAnImageIsWorthAcceptingAndTextIsNot() {
        assertTrue(acceptsDrop(listOf("application/pdf")))
        assertTrue(acceptsDrop(listOf("image/jpeg")))
        assertFalse(acceptsDrop(listOf("text/plain", "text/uri-list")))
    }

    @Test
    fun aDroppedPdfOpens() {
        assertEquals(DroppedFile.Pdf("content://docs/a.pdf"), droppedFile(listOf("content://docs/a.pdf"), types::get))
    }

    @Test
    fun aDroppedImageStamps() {
        assertEquals(DroppedFile.Image("content://media/b.png"), droppedFile(listOf("content://media/b.png"), types::get))
    }

    @Test
    fun anyOtherFileIsUnsupported() {
        assertEquals(DroppedFile.Unsupported, droppedFile(listOf("content://docs/c.txt"), types::get))
    }

    @Test
    fun onlyTheFirstFileIsActedOn() {
        val dropped = droppedFile(listOf("content://media/b.png", "content://docs/a.pdf"), types::get)
        assertEquals(DroppedFile.Image("content://media/b.png"), dropped)
    }

    @Test
    fun aRemoteOrFileUriIsNeverRead() {
        val asked = mutableListOf<String>()
        val dropped = droppedFile(listOf("https://example.com/a.pdf", "file:///sdcard/a.pdf", null)) { asked += it; "application/pdf" }

        assertNull(dropped)
        assertTrue(asked.isEmpty())
    }
}
