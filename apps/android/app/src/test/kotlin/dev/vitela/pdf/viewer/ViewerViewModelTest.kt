package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Covers the "no native PDF core" state. Without PDFium packaged,
 * [ViewerViewModel.open] returns immediately; before `canOpen` existed the
 * open buttons stayed enabled and silently swallowed every tap.
 */
class ViewerViewModelTest {
    @Test
    fun openActionsAreDisabledWhenTheNativeCoreIsMissing() {
        val state = ViewerViewModel(core = null).state.value
        assertFalse("open must be disabled with no native core", state.canOpen)
        assertTrue(
            "the status must say why opening is unavailable, got: ${state.status}",
            state.status.contains("not packaged"),
        )
    }

    @Test
    fun aStateBuiltWithoutDecidingDefaultsToDisabled() {
        // The default must err towards a disabled button: a state that forgot
        // to decide should never advertise an action that cannot run.
        assertFalse(ViewerState().canOpen)
    }

    @Test
    fun cancelPasswordIsANoOpWhenNoPromptIsShowing() {
        // T-085: cancelling must never clobber unrelated state — it only
        // acts while the password prompt is actually up.
        val viewModel = ViewerViewModel(core = null)
        val before = viewModel.state.value
        viewModel.cancelPassword()
        assertEquals(before, viewModel.state.value)
    }
}
