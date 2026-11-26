package dev.vitela.pdf.document

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/**
 * Share hands another app a copy of the open document. The copy lives in a
 * private cache folder of its own, named after the document, and outlives the
 * share sheet: a target may read it well after the chooser closed.
 */
class ShareCopiesTest {
    @get:Rule
    val temporary = TemporaryFolder()

    private val hour = 60L * 60L * 1000L

    @Test
    fun thePlainNamePassesThrough() {
        assertEquals("Contract.pdf", ShareCopies.fileName("Contract.pdf"))
    }

    @Test
    fun anExistingPdfExtensionIsKeptWhateverItsCase() {
        assertEquals("Scan.PDF", ShareCopies.fileName("Scan.PDF"))
    }

    @Test
    fun theReceiverGetsTheExtensionItNeeds() {
        assertEquals("report.pdf", ShareCopies.fileName("report"))
    }

    @Test
    fun charactersAFileNameCannotHoldAreReplaced() {
        assertEquals("Q3_ final_.pdf", ShareCopies.fileName("Q3: final?.pdf"))
        assertEquals("a_b_c.pdf", ShareCopies.fileName("a\u0000b\nc.pdf"))
    }

    @Test
    fun aDisplayNameNeverEscapesTheShareFolder() {
        assertEquals(".._.._evil.pdf", ShareCopies.fileName("../../evil.pdf"))
        assertEquals(".._.._evil.pdf", ShareCopies.fileName("..\\..\\evil.pdf"))
    }

    @Test
    fun aBlankNameFallsBackToAReadableOne() {
        assertEquals("Document.pdf", ShareCopies.fileName("  "))
        assertEquals("Document.pdf", ShareCopies.fileName(""))
    }

    @Test
    fun accentedNamesStayAsTheyAre() {
        assertEquals("año ñandú.pdf", ShareCopies.fileName("año ñandú.pdf"))
    }

    @Test
    fun aLongNameIsShortenedToFitTheFileSystem() {
        val name = ShareCopies.fileName("ñ".repeat(300) + ".pdf")
        assertTrue(name.endsWith(".pdf"))
        assertTrue(name.toByteArray(Charsets.UTF_8).size <= 255)
    }

    @Test
    fun theCopyIsWrittenInAFolderOfItsOwnUnderTheDocumentsName() {
        val root = File(temporary.root, "share")

        val copy = requireNotNull(ShareCopies.write(root, "Contract.pdf", byteArrayOf(1, 2, 3), now = 0L, folderId = "one"))

        assertEquals(File(File(root, "one"), "Contract.pdf"), copy)
        assertArrayEquals(byteArrayOf(1, 2, 3), copy.readBytes())
    }

    @Test
    fun twoSharesNeverRaceOverOneFile() {
        val root = File(temporary.root, "share")
        var next = 0
        val first = requireNotNull(ShareCopies.write(root, "a.pdf", byteArrayOf(1), now = 0L) { "id${next++}" })
        val second = requireNotNull(ShareCopies.write(root, "a.pdf", byteArrayOf(2), now = 0L) { "id${next++}" })

        assertNotEquals(first, second)
        assertArrayEquals(byteArrayOf(1), first.readBytes())
        assertArrayEquals(byteArrayOf(2), second.readBytes())
    }

    @Test
    fun aShareSweepsCopiesOlderThanAnHourAndKeepsTheRest() {
        val root = File(temporary.root, "share")
        val now = 10 * hour
        val old = folderAged(root, "old", now - hour - 1)
        val recent = folderAged(root, "recent", now - hour + 1)

        ShareCopies.write(root, "a.pdf", byteArrayOf(1), now = now, folderId = "new")

        assertFalse(old.exists())
        assertTrue(recent.exists())
        assertTrue(File(root, "new").exists())
    }

    @Test
    fun theSweepLeavesLooseFilesAlone() {
        val root = File(temporary.root, "share").apply { mkdirs() }
        val loose = File(root, "loose.txt").apply { writeText("x") }
        loose.setLastModified(0L)

        ShareCopies.sweep(root, now = 10 * hour)

        assertTrue(loose.exists())
    }

    @Test
    fun theSweepOfAMissingRootIsANoOp() {
        ShareCopies.sweep(File(temporary.root, "absent"), now = 10 * hour)
    }

    @Test
    fun aFailedWriteReportsNullInsteadOfThrowing() {
        // The share root cannot be created where a file already sits.
        val blocked = File(temporary.root, "share").apply { writeText("not a folder") }

        assertNull(ShareCopies.write(blocked, "a.pdf", byteArrayOf(1), now = 0L, folderId = "one"))
    }

    @Test
    fun aSweepThatCannotListTheRootNeverBlocksTheShare() {
        val path = File(temporary.root, "share").path
        val unlistable = object : File(path) {
            override fun listFiles(): Array<File> = throw SecurityException("no read access")
        }

        ShareCopies.sweep(unlistable, now = 10 * hour)
        val copy = requireNotNull(ShareCopies.write(unlistable, "a.pdf", byteArrayOf(4), now = 10 * hour, folderId = "one"))

        assertArrayEquals(byteArrayOf(4), copy.readBytes())
    }

    private fun folderAged(root: File, name: String, modified: Long): File =
        File(root, name).apply {
            assertTrue(mkdirs())
            File(this, "a.pdf").writeBytes(byteArrayOf(0))
            assertTrue("the test file system must take the age", setLastModified(modified))
        }

    private fun ShareCopies.write(root: File, displayName: String, bytes: ByteArray, now: Long, folderId: String): File? =
        write(root, displayName, bytes, now) { folderId }
}
