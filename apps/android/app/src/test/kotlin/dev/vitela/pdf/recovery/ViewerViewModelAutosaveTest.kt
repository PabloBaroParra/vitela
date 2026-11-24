package dev.vitela.pdf.recovery

import androidx.lifecycle.ViewModelStore
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SaveSnapshot
import dev.vitela.pdf.text.UiText
import dev.vitela.pdf.text.english
import dev.vitela.pdf.viewer.AnnotationTool
import dev.vitela.pdf.viewer.ViewerDispatcherRule
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder

@OptIn(ExperimentalCoroutinesApi::class)
class ViewerViewModelAutosaveTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @get:Rule
    val temporary = TemporaryFolder()

    private val store by lazy { FaultyRecoveryStore(FileRecoveryStore(temporary.root, LiveEntries())) }
    private val core = RecoverableCore()
    private var now = 0L
    private val viewModels = ViewModelStore()
    private val viewer by lazy {
        dispatchers.viewModel(core, clock = { 1_000L }, recovery = store, elapsed = { now }).also { viewModels.put("viewer", it) }
    }

    private fun files() = temporary.root.list().orEmpty().toList()
    private fun onDisk() = temporary.root.listFiles().orEmpty().single { it.extension == "pdf" }.readText()
    private fun sidecar() = requireNotNull(RecoveryMeta.parse(temporary.root.listFiles().orEmpty().single { it.extension == "meta" }.readText()))

    private fun TestScope.open(name: String = "a.pdf") {
        viewer.open(name, byteArrayOf(1), saveTarget = "content://a")
        advanceUntilIdle()
    }

    private fun TestScope.edit() {
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        advanceUntilIdle()
        assertTrue(viewer.state.value.isDirty)
    }

    private fun TestScope.snapshot() {
        viewer.snapshotForRecovery()
        advanceUntilIdle()
    }

    private fun TestScope.openProtected() {
        open()
        edit()
        snapshot()
        assertEquals(2, files().size)
    }

    private fun TestScope.tick() {
        now += 60_000
        advanceTimeBy(60_000)
        runCurrent()
    }

    @Test
    fun unsavedWorkIsWrittenWithItsNameAndTime() = runTest {
        open("report.pdf")
        edit()
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
        assertEquals(RecoveryMeta(1_000L, "report.pdf", false), sidecar())
    }

    @Test
    fun aCleanDocumentIsNotWritten() = runTest {
        open()
        snapshot()
        assertEquals(0, core.documents.single().snapshots)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun theSameRevisionIsWrittenOnce() = runTest {
        openProtected()
        snapshot()
        assertEquals(1, store.writes)
        edit()
        snapshot()
        assertEquals(2, store.writes)
        assertEquals("one entry, replaced", "%PDF-1.7 2", onDisk())
    }

    @Test
    fun theTimerWritesEveryMinuteOnceStarted() = runTest {
        open()
        edit()
        advanceTimeBy(120_000)
        runCurrent()
        assertEquals("not before production starts it", 0, store.writes)

        viewer.startAutosave()
        viewer.startAutosave()
        advanceTimeBy(59_999)
        runCurrent()
        assertEquals(0, store.writes)
        advanceTimeBy(2)
        runCurrent()
        assertEquals(1, store.writes)
        viewModels.clear()
    }

    @Test
    fun encryptedContentIsNeverWritten() = runTest {
        open()
        core.documents.single().encrypted = true
        edit()
        snapshot()
        assertEquals("not even serialized", 0, core.documents.single().snapshots)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun pagesFromAnEncryptedPdfRemoveTheEarlierCopy() = runTest {
        openProtected()
        core.documents.single().encrypted = true
        edit()
        snapshot()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aCoreThatDoesNotAnswerCountsAsEncrypted() {
        val silent = object : PdfDocument {
            override val pageCount = 0
            override val pageSizes = emptyList<PageSize>()
            override fun renderPage(pageIndex: Int, dpi: Int) = PdfCoreResult.Failure(PdfCoreError.WrongPassword)
            override fun search(query: String) = PdfCoreResult.Failure(PdfCoreError.WrongPassword)
            override fun close() = Unit
        }
        assertTrue(silent.holdsEncryptedContent())
    }

    @Test
    fun savingForgetsTheEntry() = runTest {
        openProtected()
        viewer.confirmSaved(requireNotNull(viewer.saveSnapshot()))
        advanceUntilIdle()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aSaveOfAnOlderRevisionKeepsTheEntry() = runTest {
        openProtected()
        val saved = requireNotNull(viewer.saveSnapshot())
        edit()
        viewer.confirmSaved(saved)
        advanceUntilIdle()
        assertEquals(2, files().size)
    }

    @Test
    fun workAfterASaveIsProtectedAgain() = runTest {
        openProtected()
        viewer.confirmSaved(requireNotNull(viewer.saveSnapshot()))
        edit()
        snapshot()
        assertEquals("%PDF-1.7 2", onDisk())
    }

    @Test
    fun closingWithDiscardForgetsTheEntryAndAskingDoesNot() = runTest {
        openProtected()
        viewer.closeDocument()
        advanceUntilIdle()
        assertNotNull(viewer.state.value.pendingClose)
        assertEquals(2, files().size)
        viewer.confirmClose()
        advanceUntilIdle()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aDiscardCountsEvenIfTheActivityFinishesWhileTheDocumentCloses() = runTest {
        openProtected()
        viewer.closeDocument()
        advanceUntilIdle()
        viewer.confirmClose()
        viewModels.clear()
        advanceUntilIdle()
        assertEquals(emptyList<String>(), files())
        assertEquals(1, core.documents.single().closes)
    }

    @Test
    fun aConfirmedReplacementForgetsTheEntryOnceInstalled() = runTest {
        openProtected()
        viewer.open("b.pdf", byteArrayOf(2))
        advanceUntilIdle()
        assertEquals("only asked", 2, files().size)
        viewer.confirmReplacement()
        advanceUntilIdle()
        assertEquals("b.pdf", viewer.state.value.title)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aReplacementThatFailsToOpenKeepsTheOldDocumentsEntryAndName() = runTest {
        openProtected()
        for (failure in listOf(PdfCoreError.Failed(UiText.Verbatim("broken")), PdfCoreError.PasswordRequired)) {
            core.failure = failure
            viewer.open("b.pdf", byteArrayOf(2))
            viewer.confirmReplacement()
            advanceUntilIdle()
            assertEquals(2, files().size)
            edit()
            snapshot()
            assertEquals("a.pdf", sidecar().name)
        }
        viewer.cancelPassword()
        snapshot()
        assertEquals(2, files().size)
    }

    @Test
    fun aForgetDuringAWriteDoesNotLeaveTheEntryBehind() = runTest {
        open()
        edit()
        val saved = requireNotNull(viewer.saveSnapshot())
        // Everything the save can do happens while the write is still on its way to the disk.
        store.duringWrite = {
            viewer.confirmSaved(saved)
            testScheduler.runCurrent()
        }
        snapshot()
        assertEquals(1, store.writes)
        assertFalse(viewer.state.value.isDirty)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aForgetBeforeTheWriteStartsStopsIt() = runTest {
        openProtected()
        val document = viewer.state.value.documentId
        fun saved() = SaveSnapshot(ByteArray(0), document, viewer.state.value.revision)
        store.duringDelete = {
            store.duringDelete = {}
            // New work is serialized, waits behind this deletion, and is saved meanwhile.
            viewer.setAnnotationTool(AnnotationTool.Highlight)
            viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
            testScheduler.runCurrent()
            viewer.snapshotForRecovery()
            testScheduler.runCurrent()
            assertEquals(2, core.documents.single().snapshots)
            viewer.confirmSaved(saved())
            testScheduler.runCurrent()
        }
        viewer.confirmSaved(saved())
        advanceUntilIdle()
        assertFalse(viewer.state.value.isDirty)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aForgetSurvivesTheActivityBeingFinished() = runTest {
        openProtected()
        viewer.confirmSaved(requireNotNull(viewer.saveSnapshot()))
        viewModels.clear()
        advanceUntilIdle()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun anEditMadeDuringASnapshotIsWrittenNextTime() = runTest {
        open()
        edit()
        val document = core.documents.single()
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        document.duringSnapshot = {
            document.duringSnapshot = {}
            viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        }
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
        assertEquals(2, document.edits)
        snapshot()
        assertEquals("%PDF-1.7 2", onDisk())
    }

    @Test
    fun anEditMadeDuringTheDiskWriteIsWrittenNextTime() = runTest {
        open()
        edit()
        store.duringWrite = {
            store.duringWrite = {}
            viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
            testScheduler.runCurrent()
            assertEquals(2, core.documents.single().edits)
        }
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
        snapshot()
        assertEquals("%PDF-1.7 2", onDisk())
    }

    @Test
    fun aFailedWriteIsRetriedAfterItsBackoffAndReportedOnce() = runTest {
        open()
        edit()
        store.writeFails = true
        viewer.startAutosave()
        tick()
        assertEquals(1, store.writes)
        assertTrue(viewer.state.value.status.english(), viewer.state.value.status.english().startsWith("Autosave is unavailable"))

        viewer.refuseDrop(UiText.Verbatim("something else"))
        tick()
        assertEquals(2, store.writes)
        assertEquals("something else", viewer.state.value.status.english())
        // Two failures in a row wait two minutes.
        tick()
        assertEquals(2, store.writes)
        store.writeFails = false
        tick()
        assertEquals(3, store.writes)
        assertEquals("%PDF-1.7 1", onDisk())
        viewModels.clear()
    }

    @Test
    fun aDocumentThatCannotBeSerializedIsRetried() = runTest {
        open()
        edit()
        core.documents.single().snapshotFails = true
        snapshot()
        assertEquals(emptyList<String>(), files())
        core.documents.single().snapshotFails = false
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
    }

    @Test
    fun goingToTheBackgroundDoesNotWaitOutTheBackoff() = runTest {
        open()
        edit()
        store.writeFails = true
        snapshot()
        store.writeFails = false
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
    }

    @Test
    fun goingToTheBackgroundTwiceWritesOneEntry() = runTest {
        open()
        edit()
        viewer.snapshotForRecovery()
        viewer.snapshotForRecovery()
        advanceUntilIdle()
        assertEquals(1, store.writes)
        assertEquals(2, files().size)
    }

    @Test
    fun beingClearedKeepsTheEntryAndOffersItAgain() = runTest {
        openProtected()
        assertNull("live", store.claimNewest(emptySet()))
        viewModels.clear()
        advanceUntilIdle()
        assertEquals(2, files().size)
        assertNotNull("no longer live", store.claimNewest(emptySet()))
    }

    @Test
    fun aClearRightAfterAStopStillWritesTheLatestEdit() = runTest {
        openProtected()
        edit()
        viewer.snapshotForRecovery()
        // Cleared before the encryption question was answered.
        viewModels.clear()
        assertEquals("the snapshot needs it", 0, core.documents.single().closes)
        advanceUntilIdle()
        assertEquals("%PDF-1.7 2", onDisk())
        assertEquals(1, core.documents.single().closes)
        assertNotNull("released", store.claimNewest(emptySet()))
    }

    @Test
    fun aClearWhileTheStopWaitsForTheLaneStillWritesTheLatestEdit() = runTest {
        openProtected()
        edit()
        viewer.search("holds the lane")
        viewer.snapshotForRecovery()
        viewModels.clear()
        advanceUntilIdle()
        assertEquals("%PDF-1.7 2", onDisk())
        assertEquals(1, core.documents.single().closes)
        assertNotNull("released", store.claimNewest(emptySet()))
    }

    @Test
    fun aClearWhileSerializingClosesTheHandleOnceAfterwards() = runTest {
        open()
        edit()
        val document = core.documents.single()
        document.duringSnapshot = {
            viewModels.clear()
            assertEquals(0, document.closes)
        }
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
        assertEquals(1, document.closes)
    }

    @Test
    fun aClearedSnapshotStillRefusesEncryptedContent() = runTest {
        openProtected()
        edit()
        core.documents.single().encrypted = true
        viewer.snapshotForRecovery()
        viewModels.clear()
        advanceUntilIdle()
        assertEquals(1, core.documents.single().snapshots)
        assertEquals(emptyList<String>(), files())
        assertEquals(1, core.documents.single().closes)
    }

    @Test
    fun aFailedSnapshotStillClosesTheHandleOfAClearedViewer() = runTest {
        openProtected()
        edit()
        core.documents.single().snapshotFails = true
        viewer.snapshotForRecovery()
        viewModels.clear()
        advanceUntilIdle()
        assertEquals("%PDF-1.7 1", onDisk())
        assertEquals(1, core.documents.single().closes)
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun aPlainClearClosesAtOnceAndNothingStartsAfterIt() = runTest {
        openProtected()
        edit()
        viewModels.clear()
        assertEquals(1, core.documents.single().closes)
        snapshot()
        assertNull(viewer.saveSnapshot())
        assertEquals("%PDF-1.7 1", onDisk())
        assertEquals(1, core.documents.single().closes)
    }

    /** The handle is the new document's but the state is still the old one's. */
    @Test
    fun aClearInTheMiddleOfAReplacementNeverWritesTheNewDocumentAsTheOld() = runTest {
        openProtected()
        edit()
        core.duringPageSizes = {
            core.duringPageSizes = {}
            viewer.snapshotForRecovery()
            viewModels.clear()
        }
        viewer.open("b.pdf", byteArrayOf(2))
        viewer.confirmReplacement()
        advanceUntilIdle()
        assertEquals(1, store.writes)
        assertEquals(listOf(1, 1), core.documents.map { it.closes })
    }

    @Test
    fun runningOutOfMemoryWhileWritingIsAFailure() = runTest {
        open()
        edit()
        store.outOfMemory = true
        snapshot()
        assertEquals(AUTOSAVE_UNAVAILABLE, viewer.state.value.status)
        store.outOfMemory = false
        snapshot()
        assertEquals("%PDF-1.7 1", onDisk())
    }

    @Test
    fun bytesTakenBeforeTheClearStillLandAndTheEntryStaysLiveUntilThen() = runTest {
        openProtected()
        edit()
        store.duringWrite = {
            viewModels.clear()
            assertNull("still being written", store.claimNewest(emptySet()))
        }
        snapshot()
        assertEquals("%PDF-1.7 2", onDisk())
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun theSignatureFlagIsRecorded() = runTest {
        open()
        core.documents.single().breaksSignatures = true
        edit()
        snapshot()
        assertTrue(sidecar().breaksSignatures)
    }

    @Test
    fun twoViewersInOneProcessKeepTwoEntries() = runTest {
        openProtected()
        val other = dispatchers.viewModel(RecoverableCore(), recovery = store)
        other.open("b.pdf", byteArrayOf(1))
        advanceUntilIdle()
        other.setAnnotationTool(AnnotationTool.Highlight)
        other.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        advanceUntilIdle()
        other.snapshotForRecovery()
        advanceUntilIdle()
        assertEquals(4, files().size)

        other.confirmSaved(requireNotNull(other.saveSnapshot()))
        advanceUntilIdle()
        assertEquals("a.pdf", sidecar().name)
    }
}
