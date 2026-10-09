package dev.vitela.pdf.viewer

import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * A hardware Enter in a box that commits on focus loss is the box's alone: both
 * halves are swallowed, and focus is let go only once the key is back up, so no
 * half of the press can reach whatever takes focus next.
 */
class EnterKeyTest {
    @Test
    fun enterDownIsSwallowedWithoutLettingFocusGo() {
        assertEquals(EnterKeyStep.Swallow, enterKeyStep(Key.Enter, KeyEventType.KeyDown))
        assertEquals(EnterKeyStep.Swallow, enterKeyStep(Key.NumPadEnter, KeyEventType.KeyDown))
    }

    @Test
    fun enterUpLetsFocusGo() {
        assertEquals(EnterKeyStep.ReleaseFocus, enterKeyStep(Key.Enter, KeyEventType.KeyUp))
        assertEquals(EnterKeyStep.ReleaseFocus, enterKeyStep(Key.NumPadEnter, KeyEventType.KeyUp))
    }

    @Test
    fun otherKeysAreLeftToTheTextField() {
        assertEquals(EnterKeyStep.PassThrough, enterKeyStep(Key.A, KeyEventType.KeyDown))
        assertEquals(EnterKeyStep.PassThrough, enterKeyStep(Key.Tab, KeyEventType.KeyUp))
        assertEquals(EnterKeyStep.PassThrough, enterKeyStep(Key.Backspace, KeyEventType.KeyDown))
    }
}
