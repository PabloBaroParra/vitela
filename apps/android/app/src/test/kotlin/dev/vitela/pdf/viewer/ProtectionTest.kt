package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Every sentence Protect shows, worded as the Windows shell words it. */
class ProtectionTest {
    @Test
    fun theOpenPasswordIsAskedForFirst() {
        assertEquals("Enter the password the document will ask for when it is opened.", protectionPasswordProblem("", ""))
        assertEquals("Enter the password the document will ask for when it is opened.", protectionPasswordProblem("", "owner"))
    }

    @Test
    fun thePermissionsPasswordIsRequired() {
        assertEquals("Enter the permissions password.", protectionPasswordProblem("user", ""))
    }

    @Test
    fun theTwoPasswordsMustDiffer() {
        assertEquals("The two passwords must be different.", protectionPasswordProblem("same", "same"))
    }

    @Test
    fun twoDifferentPasswordsAreAccepted() {
        assertNull(protectionPasswordProblem("user", "owner"))
    }

    @Test
    fun theSuggestedNameKeepsTheStemAndMarksTheProtection() {
        assertEquals("report-protected.pdf", protectedFileName("report.pdf"))
        assertEquals("Scan-protected.pdf", protectedFileName("Scan.PDF"))
        assertEquals("Document-protected.pdf", protectedFileName(" "))
    }
}
