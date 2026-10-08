package dev.vitela.pdf.home

import org.junit.Assert.assertEquals
import org.junit.Test

class HomeVersionTest {
    @Test
    fun aReleaseBuildShowsItsTagVersion() {
        assertEquals("Vitela 0.1.0-beta.3", homeVersionLabel("0.1.0-beta.3"))
    }

    @Test
    fun aBuildWithoutAVersionSaysItIsADevelopmentOne() {
        assertEquals("Vitela dev", homeVersionLabel(null))
        assertEquals("Vitela dev", homeVersionLabel(" "))
    }
}
