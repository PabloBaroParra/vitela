package dev.vitela.pdf.viewer

import androidx.annotation.StringRes
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.TextStyleToggle

/** Each toggle's word, in its own style, so the chip says what it does before it is pressed. */
@get:StringRes
private val TextStyleToggle.label: Int
    get() = when (this) {
        TextStyleToggle.Bold -> R.string.content_text_bold
        TextStyleToggle.Italic -> R.string.content_text_italic
        TextStyleToggle.Underline -> R.string.content_text_underline
    }

@Composable
private fun StyledLabel(style: TextStyleToggle) = Text(
    stringResource(style.label),
    fontWeight = if (style == TextStyleToggle.Bold) FontWeight.Bold else null,
    fontStyle = if (style == TextStyleToggle.Italic) FontStyle.Italic else null,
    textDecoration = if (style == TextStyleToggle.Underline) TextDecoration.Underline else null,
)

/**
 * Bold, Italic and Underline for existing text, each pressed when the first run
 * of the target has it. [buttons] null — a look the core could not read — leaves
 * all three off. A style that is the run's own font stays pressed and enabled:
 * a press on it is refused with the reason, never faked, which a disabled chip
 * could not tell the reader.
 */
@Composable
internal fun TextStyleToggles(buttons: TextStyleButtons?, onPress: (TextStyleToggle) -> Unit, modifier: Modifier = Modifier) {
    Row(modifier, horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        TextStyleToggle.entries.forEach { style -> TextStyleChip(style, buttons, onPress) }
    }
}

/** One toggle as a chip, for a row of its own or the context chips. */
@Composable
internal fun TextStyleChip(style: TextStyleToggle, buttons: TextStyleButtons?, onPress: (TextStyleToggle) -> Unit) {
    FilterChip(
        selected = buttons?.look?.has(style) == true,
        enabled = buttons != null,
        onClick = { onPress(style) },
        label = { StyledLabel(style) },
    )
}
