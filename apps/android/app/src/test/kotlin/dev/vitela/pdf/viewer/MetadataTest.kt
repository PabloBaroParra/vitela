package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.DocumentInfo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class MetadataTest {
    @Test
    fun anEmptiedFieldBecomesAnAbsentKey() {
        // The core reads null as "no such key in /Info"; an empty string is a
        // distinct, deliberate state a text box must not create by accident.
        val current = DocumentInfo(title = "Report", author = "Ana")
        val draft = current.copy(author = "")
        assertEquals(DocumentInfo(title = "Report", author = null), metadataChange(current, draft))
    }

    @Test
    fun anUntouchedDraftIsNoChange() {
        val current = DocumentInfo(title = "Report", keywords = null)
        // A field the user focused and left blank reads back as "" from the text box.
        assertNull(metadataChange(current, current.copy(keywords = "")))
    }

    @Test
    fun anEditedFieldIsTheChange() {
        val current = DocumentInfo(title = "Report")
        assertEquals(DocumentInfo(title = "Final report"), metadataChange(current, current.copy(title = "Final report")))
    }

    @Test
    fun whitespaceIsKeptAsTyped() {
        // Same rule as the Windows shell: only an empty field is cleared.
        val current = DocumentInfo()
        assertEquals(DocumentInfo(subject = " "), metadataChange(current, current.copy(subject = " ")))
    }
}
