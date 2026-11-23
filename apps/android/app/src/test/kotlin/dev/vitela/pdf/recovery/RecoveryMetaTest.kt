package dev.vitela.pdf.recovery

import java.time.ZoneOffset
import java.util.Locale
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class RecoveryMetaTest {
    @Test
    fun itComesBackAsItWasWritten() {
        for (breaks in listOf(true, false)) {
            val meta = RecoveryMeta(savedAt = 1_700_000_000_123, name = "año\t1\n%+ &.pdf", breaksSignatures = breaks)
            assertEquals(meta, RecoveryMeta.parse(meta.encode()))
        }
    }

    @Test
    fun onlyAnExplicitZeroClearsTheSignatureFlag() {
        fun flag(line: String) = requireNotNull(RecoveryMeta.parse("vitela-recovery 1\nsaved\t1\nname\ta.pdf\n$line")).breaksSignatures
        assertEquals(false, flag("breaks-signatures\t0\n"))
        assertEquals(true, flag("breaks-signatures\t1\n"))
        assertEquals(true, flag("breaks-signatures\tmaybe\n"))
        assertEquals(true, flag(""))
    }

    @Test
    fun anUnknownFieldIsSkipped() {
        assertEquals("a.pdf", RecoveryMeta.parse("vitela-recovery 1\nsaved\t1\nfuture\tx\nname\ta.pdf\n")?.name)
    }

    @Test
    fun whatCannotBeOfferedDoesNotParse() {
        assertNull(RecoveryMeta.parse(""))
        assertNull(RecoveryMeta.parse("vitela-recovery 2\nsaved\t1\nname\ta.pdf\n"))
        assertNull(RecoveryMeta.parse("vitela-recovery 1\nname\ta.pdf\n"))
        assertNull(RecoveryMeta.parse("vitela-recovery 1\nsaved\tyesterday\nname\ta.pdf\n"))
        assertNull(RecoveryMeta.parse("vitela-recovery 1\nsaved\t1\n"))
        assertNull(RecoveryMeta.parse("vitela-recovery 1\nsaved\t1\nname\t%zz\n"))
    }

    @Test
    fun theSavedTimeIsWrittenForTheReader() {
        val text = savedAtText(1_700_000_000_000, ZoneOffset.UTC, Locale.US)
        assertTrue(text, text.contains("Nov 14, 2023") && text.contains("10:13"))
    }
}
