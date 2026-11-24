package dev.vitela.pdf.recovery

import java.io.File
import java.io.IOException
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

class FileRecoveryStoreTest {
    @get:Rule
    val temporary = TemporaryFolder()

    private val directory by lazy { File(temporary.root, "recovery") }
    private val store by lazy { FileRecoveryStore(directory, LiveEntries()) }

    /** The same directory as a later process sees it: nothing is live. */
    private fun nextProcess() = FileRecoveryStore(directory, LiveEntries())

    private fun files() = directory.list().orEmpty().toSet()
    private fun meta(savedAt: Long = 1, name: String = "a.pdf", breaks: Boolean = false) = RecoveryMeta(savedAt, name, breaks)

    @Test
    fun anEntryIsOfferedToTheNextProcessAndReadBack() {
        store.write(store.newKey(), PDF, meta(savedAt = 5, breaks = true))

        val next = nextProcess()
        val orphan = requireNotNull(next.claimNewest(emptySet()))
        assertEquals(RecoveryMeta(5, "a.pdf", true), orphan.meta)
        assertArrayEquals(PDF, next.read(orphan.key))
    }

    @Test
    fun aMissingDirectoryHoldsNothing() = assertNull(store.claimNewest(emptySet()))

    @Test
    fun aLiveEntryIsNeverOfferedAndAReleasedOneIs() {
        val live = LiveEntries()
        val mine = FileRecoveryStore(directory, live)
        val other = FileRecoveryStore(directory, live)
        val key = mine.newKey()
        mine.write(key, PDF, meta())

        assertNull(other.claimNewest(emptySet()))
        mine.release(key)
        assertEquals(key, other.claimNewest(emptySet())?.key)
        assertNull("claimed by the other instance now", mine.claimNewest(emptySet()))
    }

    @Test
    fun theNewestIsOfferedFirstAndOnlyItIsClaimed() {
        store.write(store.newKey(), PDF, meta(savedAt = 1, name = "old.pdf"))
        store.write(store.newKey(), PDF, meta(savedAt = 2, name = "new.pdf"))

        val next = nextProcess()
        assertEquals("new.pdf", next.claimNewest(emptySet())?.meta?.name)
        assertEquals("old.pdf", next.claimNewest(emptySet())?.meta?.name)
        assertNull(next.claimNewest(emptySet()))
    }

    @Test
    fun aSkippedEntryIsKeptButNotOffered() {
        val key = store.newKey()
        store.write(key, PDF, meta())
        assertNull(nextProcess().claimNewest(setOf(key)))
        assertEquals(2, files().size)
    }

    @Test
    fun deletingRemovesEveryFile() {
        val key = store.newKey()
        store.write(key, PDF, meta())
        store.delete(key)
        assertEquals(emptySet<String>(), files())
    }

    @Test
    fun aWriteLeavesNoTemporaryBehind() {
        val key = store.newKey()
        repeat(2) { store.write(key, PDF, meta()) }
        assertEquals(setOf("${key.value}.pdf", "${key.value}.meta"), files())
    }

    @Test
    fun aFailedReplacementKeepsThePreviousBytes() {
        val key = store.newKey()
        store.write(key, PDF, meta())
        // A non-empty directory in the sidecar's place: the move onto it fails.
        val sidecar = File(directory, "${key.value}.meta")
        sidecar.delete()
        File(sidecar, "in the way").mkdirs()

        assertThrows(IOException::class.java) { store.write(key, PDF + 1, meta(breaks = true)) }

        assertArrayEquals("the sidecar that says a signature broke goes first", PDF, File(directory, "${key.value}.pdf").readBytes())
        assertFalse(files().any { it.endsWith(".partial") })
    }

    @Test
    fun aFirstWriteCutShortIsNotAnEntry() {
        val bytesOnly = store.newKey()
        directory.mkdirs()
        File(directory, "${bytesOnly.value}.pdf").writeBytes(PDF)
        val sidecarOnly = store.newKey()
        File(directory, "${sidecarOnly.value}.meta").writeText(meta(breaks = true).encode())

        assertNull(nextProcess().claimNewest(emptySet()))
        assertEquals("and is cleaned up", emptySet<String>(), files())
    }

    @Test
    fun staleTemporariesAreRemovedButNotALiveEntrysOwn() {
        val crashed = store.newKey()
        store.write(crashed, PDF, meta())
        File(directory, "${crashed.value}.pdf.partial").writeBytes(byteArrayOf(1))
        val neverLanded = store.newKey()
        File(directory, "${neverLanded.value}.pdf.partial").writeBytes(byteArrayOf(1))
        File(directory, "unrelated.txt").writeText("left alone")

        val next = nextProcess()
        val writing = next.newKey()
        File(directory, "${writing.value}.pdf.partial").writeBytes(byteArrayOf(1))

        assertNotNull(next.claimNewest(emptySet()))
        assertEquals(
            setOf("${crashed.value}.pdf", "${crashed.value}.meta", "${writing.value}.pdf.partial", "unrelated.txt"),
            files(),
        )
    }

    @Test
    fun aSidecarThatDoesNotParseIsDiscarded() {
        val key = store.newKey()
        store.write(key, PDF, meta())
        File(directory, "${key.value}.meta").writeText("garbage")

        assertNull(nextProcess().claimNewest(emptySet()))
        assertEquals(emptySet<String>(), files())
    }

    @Test
    fun aSidecarThatCannotBeReadIsKept() {
        val key = store.newKey()
        store.write(key, PDF, meta())
        val sidecar = File(directory, "${key.value}.meta")
        sidecar.delete()
        File(sidecar, "unreadable").mkdirs()

        val next = nextProcess()
        assertNull(next.claimNewest(emptySet()))
        assertTrue(File(directory, "${key.value}.pdf").exists())
        // Not left claimed either.
        sidecar.deleteRecursively()
        sidecar.writeText(meta().encode())
        assertNotNull(next.claimNewest(emptySet()))
    }

    @Test
    fun bytesThatAreNotAPdfAreCorrupt() {
        val key = store.newKey()
        for (bytes in listOf(ByteArray(0), "hello".toByteArray())) {
            store.write(key, bytes, meta())
            assertThrows(CorruptRecoveryException::class.java) { store.read(key) }
        }
    }

    /** Readers accept a PDF behind a little junk, and an incremental save keeps whatever prefix the original had. */
    @Test
    fun aPdfBehindAShortPrefixIsStillOne() {
        val key = store.newKey()
        val bytes = "junk\n".toByteArray() + PDF
        store.write(key, bytes, meta())
        assertArrayEquals(bytes, store.read(key))
    }

    @Test
    fun aMissingFileIsNotAVerdictOnTheBytes() {
        val error = assertThrows(IOException::class.java) { store.read(store.newKey()) }
        assertNotEquals(CorruptRecoveryException::class.java, error.javaClass)
    }

    private companion object {
        val PDF = "%PDF-1.7 body".toByteArray()
    }
}
