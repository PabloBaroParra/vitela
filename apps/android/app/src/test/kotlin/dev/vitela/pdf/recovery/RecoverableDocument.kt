package dev.vitela.pdf.recovery

import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RecoverySnapshot
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import java.io.IOException

/** An editable document whose recovery bytes say how many edits they hold. */
internal class RecoverableDocument(var encrypted: Boolean = false, private val duringPageSizes: () -> Unit = {}) : PdfDocument {
    var edits = 0
    var snapshots = 0
    var breaksSignatures = false
    var snapshotFails = false
    var duringSnapshot: () -> Unit = {}
    var closes = 0

    private fun bytes() = "%PDF-1.7 $edits".toByteArray()

    override val pageCount = 1
    override val pageSizes: List<PageSize>
        get() = listOf(PageSize(612.0, 792.0)).also { duringPageSizes() }
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun annotations(): PdfCoreResult<AnnotationSnapshot> =
        PdfCoreResult.Success(AnnotationSnapshot(emptyList(), editingAllowed = true, canUndo = false, canRedo = false))
    override fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> {
        edits++
        return PdfCoreResult.Success(Unit)
    }
    override fun saveToBytes(): PdfCoreResult<ByteArray> = PdfCoreResult.Success(bytes())
    override fun protectionChangeAllowed() = true
    override fun protectionWillInvalidateSignatures(): PdfCoreResult<Boolean> = PdfCoreResult.Success(false)
    override fun protect(openPassword: String, permissionsPassword: String, signaturesAcknowledged: Boolean): PdfCoreResult<ByteArray> =
        PdfCoreResult.Success("protected".toByteArray())

    // A closed handle answers as the adapter does: unknown, so encrypted.
    override fun holdsEncryptedContent() = encrypted || closes > 0
    override fun recoverySnapshot(): PdfCoreResult<RecoverySnapshot> {
        snapshots++
        val taken = RecoverySnapshot(bytes(), breaksSignatures)
        duringSnapshot()
        return if (snapshotFails || closes > 0) PdfCoreResult.Failure(PdfCoreError.Failed("no")) else PdfCoreResult.Success(taken)
    }
    override fun close() {
        closes++
    }
}

internal class RecoverableCore : PdfCore {
    val documents = mutableListOf<RecoverableDocument>()
    val opened = mutableListOf<ByteArray>()
    var failure: PdfCoreError? = null
    var duringPageSizes: () -> Unit = {}
    var duringOpen: () -> Unit = {}

    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> {
        failure?.let { return PdfCoreResult.Failure(it) }
        opened += bytes
        duringOpen()
        return PdfCoreResult.Success(RecoverableDocument(duringPageSizes = { duringPageSizes() }).also { documents += it })
    }

    override fun openWithPasswords(bytes: ByteArray, openPassword: String, permissionsPassword: String) = openFromBytes(bytes, openPassword)
}

/** A store with a hook around each disk write, and the faults a disk can have. */
internal class FaultyRecoveryStore(private val delegate: RecoveryStore) : RecoveryStore by delegate {
    var writes = 0
    var writeFails = false
    var readFails = false
    var outOfMemory = false
    var duringWrite: () -> Unit = {}
    var duringDelete: () -> Unit = {}
    var afterClaim: () -> Unit = {}

    override fun delete(key: RecoveryKey) {
        duringDelete()
        delegate.delete(key)
    }

    override fun claimNewest(skip: Set<RecoveryKey>): RecoveryOrphan? = delegate.claimNewest(skip).also { afterClaim() }

    override fun write(key: RecoveryKey, bytes: ByteArray, meta: RecoveryMeta) {
        writes++
        duringWrite()
        if (outOfMemory) throw OutOfMemoryError()
        if (writeFails) throw IOException("disk full")
        delegate.write(key, bytes, meta)
    }

    override fun read(key: RecoveryKey): ByteArray {
        if (outOfMemory) throw OutOfMemoryError()
        if (readFails) throw IOException("disk trouble")
        return delegate.read(key)
    }
}
