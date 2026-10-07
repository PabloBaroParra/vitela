package dev.vitela.pdf.home

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId
import java.time.ZonedDateTime
import java.util.Locale

/** Home's Recent list on Android: the app's own, since Android has no desktop recent store to read. */
class RecentDocumentsTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val zone: ZoneId = ZoneId.of("Europe/Madrid")

    private fun at(date: LocalDate, hour: Int, minute: Int = 0): Long =
        ZonedDateTime.of(date, LocalTime.of(hour, minute), zone).toInstant().toEpochMilli()

    private fun entry(uri: String, openedAt: Long = 0L, pages: Int = 1) = RecentDocument(uri, "$uri.pdf", openedAt, pages)

    @Test
    fun rememberingPutsTheDocumentFirstOnce() {
        val list = listOf(entry("a"), entry("b"), entry("c"))

        val next = list.remembering(entry("b", openedAt = 9))

        assertEquals(listOf("b", "a", "c"), next.map { it.uri })
        assertEquals(9L, next.first().openedAt)
    }

    @Test
    fun rememberingKeepsOnlyTheNewestEight() {
        val list = (1..MAX_RECENTS).map { entry("old$it") }

        val next = list.remembering(entry("new"))

        assertEquals(MAX_RECENTS, next.size)
        assertEquals("new", next.first().uri)
        assertFalse(next.any { it.uri == "old$MAX_RECENTS" })
    }

    @Test
    fun withoutTakesOnlyThatDocumentOff() {
        assertEquals(listOf("a", "c"), listOf(entry("a"), entry("b"), entry("c")).without("b").map { it.uri })
    }

    /** Calendar days, not elapsed hours — the desktop rule. */
    @Test
    fun daysAreCalendarDaysNotElapsedHours() {
        val today = LocalDate.of(2026, 9, 1)
        val now = at(today, 0, 10)

        assertEquals(RecentDay.Today, recentDay(at(today, 0, 0), now, zone))
        assertEquals(RecentDay.Yesterday, recentDay(at(today.minusDays(1), 23, 50), now, zone))
        assertEquals(RecentDay.Earlier, recentDay(at(today.minusDays(2), 23, 50), now, zone))
    }

    @Test
    fun theSameDayOfAnotherYearIsEarlier() {
        val now = at(LocalDate.of(2026, 1, 1), 12)

        assertEquals(RecentDay.Earlier, recentDay(at(LocalDate.of(2025, 1, 1), 12), now, zone))
        assertEquals(RecentDay.Yesterday, recentDay(at(LocalDate.of(2025, 12, 31), 12), now, zone))
    }

    @Test
    fun groupsAreStackedTodayFirstAndEmptyOnesLeftOut() {
        val today = LocalDate.of(2026, 9, 10)
        val now = at(today, 18)
        val entries = listOf(entry("old", at(today.minusDays(9), 9)), entry("morning", at(today, 9)), entry("evening", at(today, 17)))

        val groups = groupedByDay(entries, now, zone)

        assertEquals(listOf(RecentDay.Today, RecentDay.Earlier), groups.map { it.first })
        assertEquals(listOf("evening", "morning"), groups.first().second.map { it.uri })
    }

    @Test
    fun theOpenedLineReadsLikeTheDesktopOne() {
        val today = LocalDate.of(2026, 9, 10)
        val now = at(today, 18)

        // German: a 24-hour clock and a numeric date, so no locale-data spacing quirks (CLDR's narrow no-break space before AM).
        assertEquals("Opened today, 10:45 · 1 page", openedText(entry("a", at(today, 10, 45), pages = 1), now, zone, Locale.GERMANY))
        assertEquals("Opened yesterday, 21:00 · 12 pages", openedText(entry("a", at(today.minusDays(1), 21), pages = 12), now, zone, Locale.GERMANY))
        assertEquals("Opened 01.09.2026 · 0 pages", openedText(entry("a", at(LocalDate.of(2026, 9, 1), 8), pages = 0), now, zone, Locale.GERMANY))
    }

    @Test
    fun theListRoundTripsEvenWithATabOrNewlineInAName() {
        val entries = listOf(
            RecentDocument("content://docs/document/1", "tab\there\nand newline.pdf", 42L, 3),
            RecentDocument("content://docs/document/2%3A%C3%B3", "Programación.pdf", 7L, 1),
        )

        assertEquals(entries, decodeRecents(encodeRecents(entries)))
    }

    @Test
    fun malformedLinesAreSkippedNotFatal() {
        val good = encodeRecents(listOf(RecentDocument("content://x/1", "a.pdf", 1L, 2)))
        val text = "\nnot enough fields\nx\t1\tcontent%3A%2F%2Fy\ty.pdf\n1\t-1\tcontent%3A%2F%2Fz\tz.pdf\n1\t1\t\tblank.pdf\n$good"

        assertEquals(listOf(RecentDocument("content://x/1", "a.pdf", 1L, 2)), decodeRecents(text))
    }

    @Test
    fun theFileStoreKeepsTheListAcrossInstances() {
        val directory = File(folder.root, RECENTS_DIRECTORY_NAME)
        val entries = listOf(entry("content://a", 5L, 2), entry("content://b", 3L, 1))

        assertTrue(FileRecentStore(directory).save(entries))

        assertEquals(entries, FileRecentStore(directory).load())
        assertFalse(File(directory, "recents.txt.partial").exists())
    }

    @Test
    fun aMissingListLoadsAsEmpty() {
        assertEquals(emptyList<RecentDocument>(), FileRecentStore(File(folder.root, "nothing")).load())
    }

    @Test
    fun aFailedWriteKeepsTheOldList() {
        val directory = File(folder.root, RECENTS_DIRECTORY_NAME)
        val store = FileRecentStore(directory)
        store.save(listOf(entry("content://a")))
        // A directory squatting on the sibling's name makes the write fail.
        File(directory, "recents.txt.partial").mkdirs()

        assertFalse(store.save(listOf(entry("content://b"))))
        assertEquals(listOf("content://a"), store.load().map { it.uri })
    }

    @Test
    fun previewsAreKeptPerDocumentAndDeletable() {
        val directory = File(folder.root, RECENTS_DIRECTORY_NAME)
        val store = FileRecentStore(directory)

        assertTrue(store.saveThumbnail("content://a", byteArrayOf(1, 2)))
        assertTrue(store.saveThumbnail("content://b", byteArrayOf(3)))
        assertArrayEquals(byteArrayOf(1, 2), store.thumbnail("content://a"))

        store.deleteThumbnail("content://a")

        assertNull(store.thumbnail("content://a"))
        assertArrayEquals(byteArrayOf(3), store.thumbnail("content://b"))
    }

    /** A preview's file name must not spell out the document it belongs to. */
    @Test
    fun aPreviewFileIsNotNamedAfterItsDocument() {
        val directory = File(folder.root, RECENTS_DIRECTORY_NAME)
        FileRecentStore(directory).saveThumbnail("content://docs/document/secret-contract", byteArrayOf(1))

        val names = directory.list().orEmpty().toList()
        assertEquals(1, names.size)
        assertFalse(names.single().contains("secret"))
        assertTrue(names.single().matches(Regex("[0-9a-f]{64}\\.png")))
    }
}
