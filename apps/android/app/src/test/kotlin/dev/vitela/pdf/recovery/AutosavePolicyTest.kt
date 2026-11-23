package dev.vitela.pdf.recovery

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AutosavePolicyTest {
    private val revision = RecoveryRevision(documentId = 3, revision = 7)

    @Test
    fun unsavedWorkNotYetOnDiskIsWritten() {
        assertEquals(AutosaveDecision.Write, autosaveDecision(AutosaveObservation(revision, unsaved = true, encrypted = false), written = null))
    }

    @Test
    fun theRevisionAlreadyOnDiskIsNotWrittenAgain() {
        assertEquals(AutosaveDecision.Wait, autosaveDecision(AutosaveObservation(revision, unsaved = true, encrypted = false), written = revision))
    }

    @Test
    fun anotherRevisionOrAnotherDocumentIsWritten() {
        val observed = AutosaveObservation(revision, unsaved = true, encrypted = false)
        assertEquals(AutosaveDecision.Write, autosaveDecision(observed, revision.copy(revision = 6)))
        assertEquals(AutosaveDecision.Write, autosaveDecision(observed, revision.copy(documentId = 2)))
    }

    @Test
    fun nothingOpenNothingUnsavedOrAnythingEncryptedIsForgotten() {
        assertEquals(AutosaveDecision.Forget, autosaveDecision(null, written = revision))
        assertEquals(AutosaveDecision.Forget, autosaveDecision(AutosaveObservation(revision, unsaved = false, encrypted = false), revision))
        assertEquals(AutosaveDecision.Forget, autosaveDecision(AutosaveObservation(revision, unsaved = true, encrypted = true), revision))
        assertEquals(AutosaveDecision.Forget, autosaveDecision(AutosaveObservation(revision, unsaved = true, encrypted = true), written = null))
    }
}

class AutosaveBackoffTest {
    @Test
    fun theDelayDoublesUpToTheCeiling() {
        assertEquals(60_000L, AutosaveBackoff.delay(1))
        assertEquals(120_000L, AutosaveBackoff.delay(2))
        assertEquals(480_000L, AutosaveBackoff.delay(4))
        assertEquals(900_000L, AutosaveBackoff.delay(5))
        assertEquals(900_000L, AutosaveBackoff.delay(Int.MAX_VALUE))
    }

    @Test
    fun aFailurePostponesTheNextAttempt() {
        val backoff = AutosaveBackoff()
        assertTrue(backoff.ready(0))
        backoff.failed(1_000)
        assertFalse(backoff.ready(60_999))
        assertTrue(backoff.ready(61_000))
    }

    @Test
    fun onlyTheFirstFailureOfAStreakIsReported() {
        val backoff = AutosaveBackoff()
        assertTrue(backoff.failed(0))
        assertFalse(backoff.failed(60_000))
        backoff.succeeded()
        assertTrue(backoff.ready(60_001))
        assertTrue(backoff.failed(60_001))
    }
}
