package dev.vitela.pdf.core

/** The three style toggles Edit content offers for existing text. */
enum class TextStyleToggle { Bold, Italic, Underline }

/**
 * How a text run looks to the reader — not how it is painted: a bold run may be
 * a bold face or a stroked outline, an italic one an italic face or a shear.
 * Mirrors the core's `TextRunLook`. [strike] is carried through untouched: no
 * toggle sets it, but a restyle must not drop it.
 */
data class TextRunLook(
    val bold: Boolean = false,
    val italic: Boolean = false,
    val underline: Boolean = false,
    val strike: Boolean = false,
) {
    fun has(style: TextStyleToggle): Boolean = when (style) {
        TextStyleToggle.Bold -> bold
        TextStyleToggle.Italic -> italic
        TextStyleToggle.Underline -> underline
    }

    fun with(style: TextStyleToggle, on: Boolean): TextRunLook = when (style) {
        TextStyleToggle.Bold -> copy(bold = on)
        TextStyleToggle.Italic -> copy(italic = on)
        TextStyleToggle.Underline -> copy(underline = on)
    }
}

/**
 * How one run looks now — a restyle queued but not saved included — and whether
 * bold or italic can come off it. Only a run painted in a bold (or italic) font
 * whose page has no plain sibling cannot: un-bolding is never faked.
 */
data class TextRunLookState(
    val look: TextRunLook,
    val canRemoveBold: Boolean = true,
    val canRemoveItalic: Boolean = true,
)

/**
 * One run's restyle as the core records it: [run] exactly as the page was last
 * read, how it looked when the toggle was pressed, and how it should look.
 */
data class TextRunRestyle(val run: ContentTextRun, val from: TextRunLook, val to: TextRunLook)
