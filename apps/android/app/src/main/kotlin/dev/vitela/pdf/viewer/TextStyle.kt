package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.ContentSelection
import dev.vitela.pdf.core.ContentTextRun
import dev.vitela.pdf.core.TextRunLook
import dev.vitela.pdf.core.TextRunLookState
import dev.vitela.pdf.core.TextRunRestyle
import dev.vitela.pdf.core.TextStyleToggle
import dev.vitela.pdf.text.UiText

/**
 * What the Bold, Italic and Underline toggles show for a target — the open
 * editor's block, or the text blocks of a selection. [look] is the first run's:
 * what each toggle shows pressed. [boldRemovable] is false only when the first
 * run is bold and some run's bold is its own font's, with no regular sibling to
 * go back to; [italicRemovable] the same for italic. Mirrors the Windows shell's
 * `TextStyleButtons`.
 */
data class TextStyleButtons(val look: TextRunLook, val boldRemovable: Boolean, val italicRemovable: Boolean) {
    /** Whether [style] shows pressed and cannot be released. */
    fun isFixed(style: TextStyleToggle): Boolean = when (style) {
        TextStyleToggle.Bold -> look.bold && !boldRemovable
        TextStyleToggle.Italic -> look.italic && !italicRemovable
        TextStyleToggle.Underline -> false
    }
}

/**
 * The toggles a selection shows, paired with the selection they were read for:
 * a selection changed since — a tap, a move, a delete — shows none until it is
 * read again, so a stale answer is never shown over it.
 */
data class SelectionTextStyle(val selection: ContentSelection, val buttons: TextStyleButtons)

/** The toggles for the selection as it stands now; null while they are unread, or it holds no text. */
internal val ContentEditState.selectionButtons: TextStyleButtons?
    get() = selectionStyle?.takeIf { it.selection == selection }?.buttons

/** Why [style] cannot be released: the run's own font is bold (or italic) and the page has no plain version of it. */
internal fun fixedStyleReason(style: TextStyleToggle): UiText = UiText.Resource(
    if (style == TextStyleToggle.Italic) R.string.error_content_text_italic_fixed else R.string.error_content_text_bold_fixed,
)

internal val TEXT_RESTYLED: UiText = UiText.Resource(R.string.status_content_text_restyled)
internal val TEXT_STYLE_UNREADABLE: UiText = UiText.Resource(R.string.error_content_text_style_unreadable)
internal val TEXT_GONE: UiText = UiText.Resource(R.string.error_content_text_gone)

/** The toggles for [states], one per run of the target in order; null for an empty target. */
internal fun textStyleButtons(states: List<TextRunLookState>): TextStyleButtons? {
    val first = states.firstOrNull()?.look ?: return null
    return TextStyleButtons(
        first,
        boldRemovable = !first.bold || states.all { it.canRemoveBold },
        italicRemovable = !first.italic || states.all { it.canRemoveItalic },
    )
}

/** What pressing a toggle records: the restyles, or the reason it records nothing. */
internal data class TextStylePress(val restyles: List<TextRunRestyle>, val refusal: UiText? = null)

/**
 * What pressing [style] records for [targets], each run with its look. A toggle
 * follows the first run: on for every run when the first lacks it, off for every
 * run when it has it. A run already that way is left out, so a mixed target is
 * evened out rather than flipped run by run. Refused, before the core, when the
 * style is the first run's own font with no plain sibling. Empty, with no
 * refusal, only for an empty target.
 */
internal fun textStylePress(targets: List<Pair<ContentTextRun, TextRunLookState>>, style: TextStyleToggle): TextStylePress {
    val buttons = textStyleButtons(targets.map { it.second }) ?: return TextStylePress(emptyList())
    val on = !buttons.look.has(style)
    if (!on && buttons.isFixed(style)) return TextStylePress(emptyList(), fixedStyleReason(style))
    return TextStylePress(
        targets.filter { (_, state) -> state.look.has(style) != on }
            .map { (run, state) -> TextRunRestyle(run, state.look, state.look.with(style, on)) },
    )
}
