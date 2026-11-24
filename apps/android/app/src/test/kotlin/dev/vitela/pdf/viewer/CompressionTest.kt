package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.CompressedCopy
import dev.vitela.pdf.core.coreSentence
import dev.vitela.pdf.text.UiText
import dev.vitela.pdf.text.english
import org.junit.Assert.assertEquals
import org.junit.Test

/** Every sentence Compress shows, worded as the Windows and GTK shells word them. */
class CompressionTest {
    @Test
    fun everyPresetIsDescribedWithBalancedAsTheDefaultOffer() {
        assertEquals(listOf(CompressPreset.Lossless, CompressPreset.Balanced, CompressPreset.Small), CompressPreset.entries)
        assertEquals(CompressPreset.Balanced, DEFAULT_COMPRESS_PRESET)
        assertEquals("Lossless", describePreset(CompressPreset.Lossless).first)
        assertEquals("Also bring images down to 150 DPI. Visibly unchanged on screen, materially smaller on disk.", describePreset(CompressPreset.Balanced).second)
        assertEquals("Small", describePreset(CompressPreset.Small).first)
    }

    @Test
    fun sizesCountInPowersOfTen() {
        assertEquals("1 byte", humanSize(1).english())
        assertEquals("999 bytes", humanSize(999).english())
        assertEquals("1 kB", humanSize(1_000).english())
        assertEquals("250 kB", humanSize(250_400).english())
        assertEquals("1.0 MB", humanSize(1_000_000).english())
        assertEquals("12.3 MB", humanSize(12_345_678).english())
    }

    @Test
    fun thePercentageIsTruncatedSoItIsOneTheFileReached() {
        assertEquals(49L, percentSmaller(copy(before = 1_000, after = 503)))
        assertEquals(0L, percentSmaller(copy(before = 0, after = 0)))
        assertEquals(50L, percentSmaller(copy(before = Long.MAX_VALUE, after = Long.MAX_VALUE / 2)))
    }

    @Test
    fun aReductionAsksForADestination() {
        assertEquals(
            "2.0 MB uncompressed → 500 kB compressed (75% smaller). Choose where to write it.",
            compressReductionSummary(copy(before = 2_000_000, after = 500_000)).english(),
        )
    }

    @Test
    fun noGainSaysNothingWasWritten() {
        assertEquals(
            "Nothing to gain: this document compresses to the same 40 kB an uncompressed save writes, so no file was written.",
            compressNoGainSummary(copy(before = 40_000, after = 40_000)).english(),
        )
    }

    @Test
    fun theWrittenSummaryRepeatsTheMeasurement() {
        assertEquals("Compressed PDF written (2.0 MB → 500 kB, 75% smaller).", compressWrittenSummary(copy(before = 2_000_000, after = 500_000)).english())
    }

    @Test
    fun aRefusalTheShellDidNotAskAboutStillReachesTheReader() {
        assertEquals(
            "Nothing to gain: this document compresses to the same 40 kB an uncompressed save writes, so no file was written. Not everything could be done: one; two.",
            compressNoGainSummary(copy(before = 40_000, after = 40_000, refusals = listOf(UiText.Verbatim("one"), UiText.Verbatim("two")))).english(),
        )
    }

    @Test
    fun aCoreClauseBecomesASentence() {
        assertEquals("This cannot be compressed.", coreSentence("this cannot be compressed").english())
    }

    @Test
    fun theSuggestedNameKeepsTheStemAndMarksTheCompression() {
        assertEquals("report-compressed.pdf", compressedFileName("report.pdf"))
        assertEquals("Scan-compressed.pdf", compressedFileName("Scan.PDF"))
        assertEquals("Document-compressed.pdf", compressedFileName(" "))
    }

    private fun copy(before: Long, after: Long, refusals: List<UiText> = emptyList()) =
        CompressedCopy(ByteArray(0), before, after, (before - after).coerceAtLeast(0), after < before, refusals)
}
