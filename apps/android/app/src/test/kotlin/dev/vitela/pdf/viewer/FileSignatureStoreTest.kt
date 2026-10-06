package dev.vitela.pdf.viewer

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

/** The remembered signature is one PNG in the app's private files, nowhere else. */
class FileSignatureStoreTest {
    @get:Rule
    val folder = TemporaryFolder()

    private val file get() = File(folder.root, SIGNATURE_FILE_NAME)
    private val store get() = FileSignatureStore(file)

    @Test
    fun nothingIsRememberedAtFirst() {
        assertNull(store.load())
    }

    @Test
    fun aSavedSignatureLoadsBackByteForByte() {
        assertTrue(store.save(byteArrayOf(1, 2, 3)))

        assertArrayEquals(byteArrayOf(1, 2, 3), FileSignatureStore(file).load())
    }

    @Test
    fun aNewSignatureReplacesTheOldOne() {
        store.save(byteArrayOf(1, 2, 3))
        store.save(byteArrayOf(9))

        assertArrayEquals(byteArrayOf(9), store.load())
        assertTrue("no temporary file may be left behind", folder.root.listFiles()!!.all { it.name == SIGNATURE_FILE_NAME })
    }

    @Test
    fun deletingForgetsIt() {
        store.save(byteArrayOf(1))

        store.delete()

        assertNull(store.load())
        assertFalse(file.exists())
    }

    @Test
    fun anEmptyFileIsNotASignature() {
        file.writeBytes(ByteArray(0))

        assertNull(store.load())
    }
}
