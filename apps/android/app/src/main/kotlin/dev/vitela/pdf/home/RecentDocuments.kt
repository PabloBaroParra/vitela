package dev.vitela.pdf.home

import java.io.File
import java.io.IOException
import java.net.URLDecoder
import java.net.URLEncoder
import java.nio.file.Files
import java.nio.file.StandardCopyOption
import java.security.MessageDigest
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.util.Locale

/**
 * One document Home lists under Recent. [uri] is the SAF document the app
 * keeps a persisted read grant on; nothing above the SAF layer interprets it.
 */
data class RecentDocument(val uri: String, val displayName: String, val openedAt: Long, val pageCount: Int)

/** How many documents the list keeps — the same eight the desktop Home shows. */
internal const val MAX_RECENTS = 8

/**
 * Android has no desktop-wide recent store to read the way Windows and Linux
 * do, so the list is the app's own. Opening puts a document first; removing
 * one takes it off for good, until the user opens it again.
 */
internal fun List<RecentDocument>.remembering(entry: RecentDocument): List<RecentDocument> =
    (listOf(entry) + filter { it.uri != entry.uri }).take(MAX_RECENTS)

internal fun List<RecentDocument>.without(uri: String): List<RecentDocument> = filter { it.uri != uri }

/** Which day-group a document falls in, in the order the groups are stacked. */
enum class RecentDay(val chip: String) { Today("Today"), Yesterday("Yesterday"), Earlier("Earlier") }

/** Calendar days, not elapsed hours: 23:50 yesterday is "Yesterday" at 00:10 today, as on the desktop. */
internal fun recentDay(openedAt: Long, now: Long, zone: ZoneId): RecentDay {
    val opened = Instant.ofEpochMilli(openedAt).atZone(zone).toLocalDate()
    val today = Instant.ofEpochMilli(now).atZone(zone).toLocalDate()
    return when (opened) {
        today -> RecentDay.Today
        today.minusDays(1) -> RecentDay.Yesterday
        else -> RecentDay.Earlier
    }
}

/** The card's "Opened …" line, with the page count the desktop appends once a preview exists. */
internal fun openedText(entry: RecentDocument, now: Long, zone: ZoneId, locale: Locale = Locale.getDefault()): String {
    val opened = Instant.ofEpochMilli(entry.openedAt).atZone(zone)
    val time = opened.format(DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale))
    val day = when (recentDay(entry.openedAt, now, zone)) {
        RecentDay.Today -> "Opened today, $time"
        RecentDay.Yesterday -> "Opened yesterday, $time"
        RecentDay.Earlier -> "Opened ${opened.toLocalDate().format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM).withLocale(locale))}"
    }
    return "$day · ${pageCountText(entry.pageCount)}"
}

internal fun pageCountText(pages: Int): String = if (pages == 1) "1 page" else "$pages pages"

/** Today, Yesterday, Earlier — each with its documents newest first, empty groups left out. */
internal fun groupedByDay(entries: List<RecentDocument>, now: Long, zone: ZoneId): List<Pair<RecentDay, List<RecentDocument>>> =
    RecentDay.entries.mapNotNull { day ->
        entries.filter { recentDay(it.openedAt, now, zone) == day }.sortedByDescending { it.openedAt }.takeIf { it.isNotEmpty() }?.let { day to it }
    }

/**
 * One line per document: `openedAt TAB pageCount TAB uri TAB name`, the last
 * two URL-encoded so a tab or newline in a provider's display name cannot
 * split a line. A line that does not parse is dropped, not fatal.
 */
internal fun encodeRecents(entries: List<RecentDocument>): String = entries.joinToString("") {
    "${it.openedAt}\t${it.pageCount}\t${URLEncoder.encode(it.uri, "UTF-8")}\t${URLEncoder.encode(it.displayName, "UTF-8")}\n"
}

internal fun decodeRecents(text: String): List<RecentDocument> = text.lineSequence().mapNotNull { line ->
    val fields = line.split('\t')
    if (fields.size != 4) return@mapNotNull null
    val openedAt = fields[0].toLongOrNull() ?: return@mapNotNull null
    val pageCount = fields[1].toIntOrNull()?.takeIf { it >= 0 } ?: return@mapNotNull null
    val uri = runCatching { URLDecoder.decode(fields[2], "UTF-8") }.getOrNull()?.takeIf { it.isNotBlank() } ?: return@mapNotNull null
    val name = runCatching { URLDecoder.decode(fields[3], "UTF-8") }.getOrNull() ?: return@mapNotNull null
    RecentDocument(uri, name, openedAt, pageCount)
}.distinctBy { it.uri }.take(MAX_RECENTS).toList()

/** Where the list and its first-page previews live. Called off the main thread — every method touches storage. */
interface RecentStore {
    fun load(): List<RecentDocument>

    /** Replaces the list; false when it could not be written (the old one is kept). */
    fun save(entries: List<RecentDocument>): Boolean

    /** The PNG preview of [uri]'s first page, or null when there is none. */
    fun thumbnail(uri: String): ByteArray?

    fun saveThumbnail(uri: String, png: ByteArray): Boolean

    fun deleteThumbnail(uri: String)
}

/** Remembers nothing — for a ViewModel built without app storage. */
object NoRecentStore : RecentStore {
    override fun load(): List<RecentDocument> = emptyList()
    override fun save(entries: List<RecentDocument>) = false
    override fun thumbnail(uri: String): ByteArray? = null
    override fun saveThumbnail(uri: String, png: ByteArray) = false
    override fun deleteThumbnail(uri: String) = Unit
}

/** The directory name in the app's private files; `data_extraction_rules.xml` excludes it by this name. */
internal const val RECENTS_DIRECTORY_NAME = "recents"

/**
 * The list and its previews in [directory], inside the app's private files.
 * Every file is written to a sibling and moved over, so a crash mid-write
 * leaves the old list, not half of the new one.
 */
class FileRecentStore(private val directory: File) : RecentStore {
    private val list = File(directory, "recents.txt")

    override fun load(): List<RecentDocument> = try {
        if (list.isFile) decodeRecents(list.readText()) else emptyList()
    } catch (_: IOException) {
        emptyList()
    }

    override fun save(entries: List<RecentDocument>): Boolean = replace(list, encodeRecents(entries).toByteArray())

    override fun thumbnail(uri: String): ByteArray? = try {
        thumbnailFile(uri).takeIf { it.isFile }?.readBytes()?.takeIf { it.isNotEmpty() }
    } catch (_: IOException) {
        null
    }

    override fun saveThumbnail(uri: String, png: ByteArray): Boolean = replace(thumbnailFile(uri), png)

    override fun deleteThumbnail(uri: String) {
        thumbnailFile(uri).delete()
    }

    /** Named by a digest: a URI is not a file name, and the name must not reveal it either. */
    private fun thumbnailFile(uri: String): File {
        val digest = MessageDigest.getInstance("SHA-256").digest(uri.toByteArray()).joinToString("") { "%02x".format(it) }
        return File(directory, "$digest.png")
    }

    private fun replace(target: File, bytes: ByteArray): Boolean {
        val partial = File(directory, "${target.name}.partial")
        return try {
            directory.mkdirs()
            partial.writeBytes(bytes)
            Files.move(partial.toPath(), target.toPath(), StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE)
            true
        } catch (_: IOException) {
            partial.delete()
            false
        }
    }
}
