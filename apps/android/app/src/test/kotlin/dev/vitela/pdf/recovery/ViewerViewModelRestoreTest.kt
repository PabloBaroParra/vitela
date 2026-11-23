package dev.vitela.pdf.recovery

import androidx.lifecycle.ViewModelStore
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.home.RecentDocument
import dev.vitela.pdf.home.RecentStore
import dev.vitela.pdf.viewer.AnnotationTool
import dev.vitela.pdf.viewer.ViewerDispatcherRule
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
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
class ViewerViewModelRestoreTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @get:Rule
    val temporary = TemporaryFolder()

    private val directory by lazy { temporary.newFolder("recovery") }
    private val live = LiveEntries()
    private val store by lazy { FaultyRecoveryStore(FileRecoveryStore(directory, live)) }
    private val core = RecoverableCore()
    private val recents = CountingRecents()
    private val viewModels = ViewModelStore()
    private val viewer by lazy { dispatchers.viewModel(core, recents = recents, recovery = store).also { viewModels.put("viewer", it) } }

    private fun files() = directory.list().orEmpty().toList()

    /** What a process that died left behind. */
    private fun crashed(name: String = "a.pdf", savedAt: Long = 1, breaks: Boolean = false, bytes: String = "%PDF-1.7 $name"): RecoveryKey {
        val gone = FileRecoveryStore(directory, LiveEntries())
        return gone.newKey().also { gone.write(it, bytes.toByteArray(), RecoveryMeta(savedAt, name, breaks)) }
    }

    private fun claimedByTheNextProcess() = FileRecoveryStore(directory, LiveEntries()).claimNewest(emptySet())

    private fun TestScope.offer() {
        viewer.offerRecovery()
        advanceUntilIdle()
    }

    private fun TestScope.restore() {
        viewer.restoreRecovery()
        advanceUntilIdle()
    }

    private fun TestScope.edit() {
        viewer.setAnnotationTool(AnnotationTool.Highlight)
        viewer.placeAnnotation(0, AnnotationPoint(10.0, 10.0), AnnotationPoint(40.0, 20.0))
        advanceUntilIdle()
    }

    private fun TestScope.snapshot() {
        viewer.snapshotForRecovery()
        advanceUntilIdle()
    }

    private fun TestScope.closeDiscarding() {
        viewer.closeDocument()
        advanceUntilIdle()
        viewer.confirmClose()
        advanceUntilIdle()
    }

    @Test
    fun nothingLeftBehindOffersNothing() = runTest {
        offer()
        assertNull(viewer.recoveryOffer.value)
    }

    @Test
    fun theOfferNamesTheDocumentAndItsTime() = runTest {
        crashed("report.pdf", savedAt = 42)
        offer()
        assertEquals(RecoveryOffer("report.pdf", 42), viewer.recoveryOffer.value)
        assertTrue(recoveryOfferVisible(viewer.state.value))
    }

    @Test
    fun aRestoredDocumentOpensUnsavedUnderItsNameWithNowhereToWriteBack() = runTest {
        crashed("report.pdf")
        offer()
        restore()

        val state = viewer.state.value
        assertEquals("report.pdf", state.title)
        assertTrue(state.isDirty)
        assertNull(state.saveTarget)
        assertNull(viewer.recoveryOffer.value)
        assertNull(viewer.inPlaceSave())
        assertEquals("%PDF-1.7 report.pdf", core.opened.single().decodeToString())
        assertEquals("not a file the user opened", 0, recents.saves)
    }

    @Test
    fun restoringTakesTheEntryOver() = runTest {
        val key = crashed()
        offer()
        restore()
        assertEquals("kept, not deleted", setOf("${key.value}.pdf", "${key.value}.meta"), files().toSet())
        assertNull("and live", store.claimNewest(emptySet()))

        snapshot()
        assertEquals("it already holds this revision", 0, store.writes)
        edit()
        snapshot()
        assertEquals("the same entry, replaced", setOf("${key.value}.pdf", "${key.value}.meta"), files().toSet())
        assertEquals(1, store.writes)

        viewer.confirmSaved(requireNotNull(viewer.saveSnapshot()))
        advanceUntilIdle()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aRestoredDocumentSurvivesAnotherCrashAndAnotherClear() = runTest {
        crashed()
        offer()
        restore()
        assertNotNull(claimedByTheNextProcess())
        viewModels.clear()
        advanceUntilIdle()
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun discardingARestoredDocumentDeletesItsEntry() = runTest {
        crashed()
        offer()
        restore()
        closeDiscarding()
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun aBrokenSignatureStillRefusesTheSaveAndStaysRecorded() = runTest {
        crashed(breaks = true)
        offer()
        restore()
        assertNull(viewer.saveSnapshot())
        assertTrue(viewer.state.value.status.contains("signature"))
        assertTrue(viewer.state.value.isDirty)

        edit()
        snapshot()
        assertTrue(requireNotNull(claimedByTheNextProcess()).meta.breaksSignatures)
    }

    @Test
    fun aBrokenSignatureIsKeptThroughProtectAsTheMessageSays() = runTest {
        crashed(breaks = true)
        offer()
        restore()
        assertEquals(RECOVERED_SIGNATURE_BROKEN, viewer.state.value.status)

        viewer.openProtect()
        advanceUntilIdle()
        assertTrue(requireNotNull(viewer.state.value.protect).signaturesWillBreak)
        assertNotNull(viewer.confirmProtect("open", "permissions"))
        var written = ""
        viewer.writeProtected("kept.pdf", null) { written = it.decodeToString(); true }
        advanceUntilIdle()
        assertEquals("protected", written)
        assertFalse(viewer.state.value.isDirty)
        assertEquals(emptyList<String>(), files())
    }

    @Test
    fun anUnbrokenOneSavesAsUsual() = runTest {
        crashed(breaks = false)
        offer()
        restore()
        assertEquals(RECOVERED, viewer.state.value.status)
        assertNotNull(viewer.saveSnapshot())
    }

    @Test
    fun discardDeletesAndMovesToTheNextOne() = runTest {
        crashed("new.pdf", savedAt = 2)
        crashed("old.pdf", savedAt = 1)
        offer()
        assertEquals("new.pdf", viewer.recoveryOffer.value?.name)
        viewer.discardRecovery()
        advanceUntilIdle()
        assertEquals("old.pdf", viewer.recoveryOffer.value?.name)
        assertEquals(2, files().size)
    }

    @Test
    fun laterKeepsEverythingAndAsksNoMoreThisLaunch() = runTest {
        crashed("new.pdf", savedAt = 2)
        crashed("old.pdf", savedAt = 1)
        offer()
        viewer.keepRecoveryForLater()
        offer()
        assertNull(viewer.recoveryOffer.value)
        assertEquals(4, files().size)
        assertEquals("released", "new.pdf", store.claimNewest(emptySet())?.meta?.name)
    }

    @Test
    fun theNextOneWaitsForTheRestoredDocumentToBeClosed() = runTest {
        crashed("new.pdf", savedAt = 2)
        crashed("old.pdf", savedAt = 1)
        offer()
        restore()
        offer()
        assertNull("never over an open document", viewer.recoveryOffer.value)
        closeDiscarding()
        offer()
        assertEquals("old.pdf", viewer.recoveryOffer.value?.name)
    }

    @Test
    fun aLiveEntryIsNotOffered() = runTest {
        val other = FileRecoveryStore(directory, live)
        other.write(other.newKey(), "%PDF-1.7".toByteArray(), RecoveryMeta(1, "theirs.pdf", false))
        offer()
        assertNull(viewer.recoveryOffer.value)
        assertEquals(2, files().size)
    }

    @Test
    fun aRestoreNeverReplacesADocumentOpenedMeanwhile() = runTest {
        crashed()
        offer()
        viewer.open("launch.pdf", byteArrayOf(1))
        viewer.restoreRecovery()
        advanceUntilIdle()
        assertEquals("launch.pdf", viewer.state.value.title)
        assertFalse(recoveryOfferVisible(viewer.state.value))
        assertEquals("kept for when Home is back", "a.pdf", viewer.recoveryOffer.value?.name)
        assertEquals(1, core.opened.size)
        assertEquals(2, files().size)
    }

    @Test
    fun anOpenRightAfterARestoreAsksBeforeReplacingIt() = runTest {
        crashed()
        offer()
        viewer.restoreRecovery()
        viewer.open("launch.pdf", byteArrayOf(1))
        advanceUntilIdle()
        assertEquals("launch.pdf", viewer.state.value.pendingReplacementTitle)
        assertEquals("a.pdf", viewer.state.value.title)
        assertEquals(2, files().size)
    }

    @Test
    fun corruptBytesAreDiscarded() = runTest {
        crashed(bytes = "not a pdf")
        offer()
        restore()
        assertEquals(emptyList<String>(), files())
        assertNull(viewer.recoveryOffer.value)
        assertTrue(viewer.state.value.status.contains("discarded"))
        assertFalse(viewer.state.value.isLoading)
    }

    @Test
    fun bytesThatWantAPasswordAreNotWhatAutosaveWroteAndAreDiscarded() = runTest {
        crashed()
        core.failure = PdfCoreError.PasswordRequired
        offer()
        restore()
        assertEquals(emptyList<String>(), files())
        assertFalse(viewer.state.value.needsPassword)
    }

    @Test
    fun aReadFailureKeepsTheEntryAndMovesOn() = runTest {
        crashed("new.pdf", savedAt = 2)
        crashed("old.pdf", savedAt = 1)
        store.readFails = true
        offer()
        restore()
        assertEquals(4, files().size)
        assertEquals("old.pdf", viewer.recoveryOffer.value?.name)
        assertFalse(viewer.state.value.isLoading)
        restore()
        assertNull("not asked about in a loop", viewer.recoveryOffer.value)
    }

    @Test
    fun runningOutOfMemoryReadingTheEntryKeepsAndSkipsIt() = runTest {
        crashed()
        store.outOfMemory = true
        offer()
        restore()
        assertEquals(RECOVERY_UNREADABLE, viewer.state.value.status)
        assertEquals(2, files().size)
        store.outOfMemory = false
        offer()
        assertNull("skipped this launch", viewer.recoveryOffer.value)
    }

    @Test
    fun aDocumentOpenedAfterTheClearIsClosedAgain() = runTest {
        crashed()
        offer()
        core.duringOpen = { viewModels.clear() }
        restore()
        assertEquals(1, core.documents.single().closes)
        assertEquals(2, files().size)
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun aCoreFailureThatMayPassKeepsTheEntry() = runTest {
        crashed()
        core.failure = PdfCoreError.Failed("PDFium is unavailable.")
        offer()
        restore()
        assertEquals(2, files().size)
        assertTrue(viewer.state.value.status.contains("kept"))
        assertNotNull("released", store.claimNewest(emptySet()))
    }

    @Test
    fun beingClearedReleasesAnUnansweredOffer() = runTest {
        crashed()
        offer()
        viewModels.clear()
        advanceUntilIdle()
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun beingClearedDuringARestoreReleasesTheEntry() = runTest {
        crashed()
        offer()
        viewer.restoreRecovery()
        viewModels.clear()
        advanceUntilIdle()
        assertEquals(2, files().size)
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun beingClearedDuringTheScanLeavesNothingClaimed() = runTest {
        crashed()
        store.afterClaim = {
            store.afterClaim = {}
            viewModels.clear()
        }
        offer()
        assertNotNull(store.claimNewest(emptySet()))
    }

    @Test
    fun withoutACoreNothingIsOffered() = runTest {
        crashed()
        val bare = dispatchers.viewModel(null, recovery = store)
        bare.offerRecovery()
        advanceUntilIdle()
        assertNull(bare.recoveryOffer.value)
    }
}

private class CountingRecents : RecentStore {
    var saves = 0
    override fun load(): List<RecentDocument> = emptyList()
    override fun save(entries: List<RecentDocument>): Boolean {
        saves++
        return true
    }
    override fun thumbnail(uri: String): ByteArray? = null
    override fun saveThumbnail(uri: String, png: ByteArray) = false
    override fun deleteThumbnail(uri: String) = Unit
}
