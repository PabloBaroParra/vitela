package dev.vitela.pdf.viewer

import androidx.annotation.StringRes
import dev.vitela.pdf.R

/**
 * The three tabs under the reader's title bar. Read is a plain reader; Edit
 * owns the annotation tools and content editing; Sign owns form filling and
 * certificate signing — the desktop shells' "Fill & Sign".
 */
internal enum class ReaderMode(@StringRes val label: Int) {
    Read(R.string.reader_mode_read),
    Edit(R.string.reader_mode_edit),
    Sign(R.string.reader_mode_sign),
}

/**
 * What leaving a mode must undo, so a tool armed in one tab never acts on a
 * page tap in another: a highlighter left armed under Read would turn every
 * scroll-and-tap into an annotation the user cannot see the tool for.
 */
internal data class ModeExit(val disarmTool: Boolean, val closeContentEdit: Boolean, val closeFormFields: Boolean) {
    companion object {
        val None = ModeExit(disarmTool = false, closeContentEdit = false, closeFormFields = false)
    }
}

internal fun modeExit(from: ReaderMode, to: ReaderMode, state: ViewerState): ModeExit {
    if (from == to) return ModeExit.None
    return ModeExit(
        // Sign arms a tool too: a drawn signature waits for its tap as an image stamp.
        disarmTool = from != ReaderMode.Read && state.activeAnnotationTool != AnnotationTool.Pointer,
        closeContentEdit = from == ReaderMode.Edit && state.contentEdit != null,
        closeFormFields = from == ReaderMode.Sign && state.formFields != null,
    )
}

/** The page grid replaces the reader, so tabs that change what a page tap does have nothing to act on. */
internal fun modeBarVisible(state: ViewerState): Boolean = state.organize == null

/** No document has pages, so the reader has nothing to show: Home takes the screen instead. */
internal fun showsHome(state: ViewerState): Boolean = state.pageCount == 0

/** The current page as the user counts it, from one. */
internal fun pageNumber(state: ViewerState): Int = state.pageIndex + 1
