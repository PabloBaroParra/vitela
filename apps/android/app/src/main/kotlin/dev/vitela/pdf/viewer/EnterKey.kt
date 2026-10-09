package dev.vitela.pdf.viewer

import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalFocusManager

internal enum class EnterKeyStep { PassThrough, Swallow, ReleaseFocus }

internal fun enterKeyStep(key: Key, type: KeyEventType): EnterKeyStep = when {
    key != Key.Enter && key != Key.NumPadEnter -> EnterKeyStep.PassThrough
    type == KeyEventType.KeyUp -> EnterKeyStep.ReleaseFocus
    else -> EnterKeyStep.Swallow
}

/** Where a hardware Enter puts focus: an element that takes it and does nothing with a key. */
internal val LocalFocusParking = staticCompositionLocalOf<FocusRequester?> { null }

/**
 * A hardware Enter in a box that commits on focus loss lets focus go, as the
 * soft keyboard's Done does, and nothing else. Two traps sit in the way. Left
 * to the text field, Enter's down runs Done, and the up lands on whatever took
 * focus. And with a hardware keyboard, clearFocus does not leave focus empty:
 * it moves to the first focusable, the top bar's back button, so the key's up
 * (or the next Enter) asks "Discard unsaved changes?". So both halves are
 * swallowed here, and only on the up does focus go, to [LocalFocusParking].
 */
@Composable
internal fun Modifier.releaseFocusOnEnter(): Modifier {
    val parking = LocalFocusParking.current
    val focusManager = LocalFocusManager.current
    return onPreviewKeyEvent {
        when (enterKeyStep(it.key, it.type)) {
            EnterKeyStep.PassThrough -> false
            EnterKeyStep.Swallow -> true
            EnterKeyStep.ReleaseFocus -> {
                if (parking != null) parking.requestFocus() else focusManager.clearFocus()
                true
            }
        }
    }
}
