package dev.vitela.pdf.viewer

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.text.UiText
import dev.vitela.pdf.text.resolve

/** What Edit content asks of the ViewModel, bundled like [FormFieldActions]. */
internal class ContentEditActions(
    val onToggle: () -> Unit,
    /** The reader laid out a page while the mode is armed. */
    val onPageShown: (pageIndex: Int) -> Unit,
    /** pageIndex, tap point and reach, in PDF points. */
    val onTap: (Int, AnnotationPoint, Double) -> Unit,
    /** documentId, text: the document is the one the dialog was built for. */
    val onRetype: (Long, String) -> Unit,
    /** documentId: deletes the dialog's text block; the document is the one the dialog was built for. */
    val onDeleteText: (Long) -> Unit,
    /** documentId: swaps the retype dialog for a move armed on the next page tap. */
    val onMoveText: (Long) -> Unit,
    val onDismiss: () -> Unit,
    /** documentId, width, height as typed, in points: the document is the one the dialog was built for. */
    val onResize: (Long, String, String) -> Unit,
    val onDismissResizer: () -> Unit,
    /** documentId: swaps the resize dialog for a move armed on the next page tap. */
    val onMove: (Long) -> Unit,
    /** documentId, width, height as typed, in points: the document is the one the dialog was built for. */
    val onResizePath: (Long, String, String) -> Unit,
    val onDismissShaper: () -> Unit,
    /** documentId: swaps the shape dialog for a move armed on the next page tap. */
    val onMovePath: (Long) -> Unit,
    /** documentId: deletes the dialog's path; the document is the one the dialog was built for. */
    val onDeletePath: (Long) -> Unit,
    /** Disarms a move of a text block, an image or a path. */
    val onCancelMove: () -> Unit,
    /** documentId: deletes the dialog's image; the document is the one the dialog was built for. */
    val onDelete: (Long) -> Unit,
    /** documentId: checks the dialog's image can be replaced, then opens the image picker. */
    val onReplace: (Long) -> Unit,
    /** Arms the next page tap to place a new line of text. */
    val onAddText: () -> Unit,
    /** Opens the image picker; the chosen image arms the next page tap. */
    val onAddImage: () -> Unit,
    val onCancelInsert: () -> Unit,
    /** documentId, text, size as typed, in points: the document is the one the dialog was built for. */
    val onInsertText: (Long, String, String) -> Unit,
    val onDismissInserter: () -> Unit,
    /** pageIndex and the marquee's box, in PDF points: selects everything it touches. */
    val onMarquee: (Int, AnnotationRect) -> Unit,
    /** pageIndex, point and reach, in PDF points: a long-press that did not move selects the item under it. */
    val onPress: (Int, AnnotationPoint, Double) -> Unit,
    /** pageIndex and the drag's offset, in PDF points: moves the selection. */
    val onMoveSelection: (Int, Double, Double) -> Unit,
    /** Deletes the selection, one undo step. */
    val onDeleteSelection: () -> Unit,
    val onClearSelection: () -> Unit,
    /** Copies the selection into the clip. */
    val onCopySelection: () -> Unit,
    /** Copies the selection into the clip and deletes it, one undo step. */
    val onCutSelection: () -> Unit,
    /** Arms the next page tap to paste the held clip. */
    val onPasteContent: () -> Unit,
)

/**
 * What the retype dialog says about its actions. The save warning is only for a
 * document the core cannot checkpoint — one with imported pages, signatures or
 * encryption; elsewhere it writes the earlier edit into memory and carries on.
 * A [paragraph]'s dialog also says what a typed line break does.
 */
internal fun textDialogHelp(canCheckpoint: Boolean, paragraph: Boolean = false): UiText = UiText.Sentences(
    listOfNotNull(
        if (paragraph) UiText.Resource(R.string.content_text_help_paragraph) else null,
        UiText.Resource(R.string.content_text_help_move),
        if (canCheckpoint) null else UiText.Resource(R.string.content_text_help_save),
        UiText.Resource(R.string.content_text_help_delete),
    ),
)

/**
 * The retype dialog: the text of the whole block the tap landed in — a line, or
 * a paragraph of several — ready to change; a block painted as several runs is
 * retyped as one piece of text. A line is a one-line field; a paragraph is a
 * multi-line one, its lines on lines of their own, where Enter types a line
 * break the core keeps when it wraps the text back to the paragraph's width.
 * Keyed on the editor, so a refusal — which comes back with what was typed —
 * reopens with it rather than with the block's old text. Move leaves it for a
 * page tap that places the block, like an image's Move. Delete takes the block
 * off the page at once, like an image's Delete: it is one undoable entry.
 *
 * The column scrolls, so on a short screen with the keyboard up the field and
 * its help stay reachable rather than pushed under it.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun TextBlockEditorDialog(editor: TextBlockEditor, documentId: Long, actions: ContentEditActions) {
    var text by remember(editor) { mutableStateOf(editor.text) }
    val paragraph = editor.block.isParagraph
    AlertDialog(
        onDismissRequest = actions.onDismiss,
        title = { Text(stringResource(R.string.content_text_title)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (editor.block.substitutesFont) Text(FONT_SUBSTITUTED.resolve(), style = MaterialTheme.typography.bodySmall)
                Text(textDialogHelp(editor.canCheckpoint, paragraph).resolve(), style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    singleLine = !paragraph,
                    minLines = if (paragraph) paragraphFieldLines(editor.block.lineCount) else 1,
                    maxLines = if (paragraph) PARAGRAPH_FIELD_MAX_LINES else 1,
                    isError = editor.error != null,
                    supportingText = editor.error?.let { error -> { Text(error.resolve()) } },
                )
            }
        },
        confirmButton = { Button(onClick = { actions.onRetype(documentId, text) }) { Text(stringResource(R.string.content_text_retype)) } },
        dismissButton = {
            // Three actions beside Retype do not fit one row on a narrow phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDeleteText(documentId) }) { Text(stringResource(R.string.dialog_delete)) }
                TextButton(onClick = { actions.onMoveText(documentId) }) { Text(stringResource(R.string.dialog_move)) }
                TextButton(onClick = actions.onDismiss) { Text(stringResource(R.string.dialog_cancel)) }
            }
        },
    )
}

/** The tallest a paragraph's field grows before it scrolls, so Retype stays on a phone's screen. */
internal const val PARAGRAPH_FIELD_MAX_LINES = 6

/** How many lines a paragraph's field opens with: one per line it has, two to [PARAGRAPH_FIELD_MAX_LINES]. */
internal fun paragraphFieldLines(lineCount: Int): Int = lineCount.coerceIn(2, PARAGRAPH_FIELD_MAX_LINES)

/**
 * The resize dialog: the image's width and height in points, ready to change.
 * Its top-left corner stays put. Keyed on the resizer, so a refusal reopens
 * with what was typed, like the retype dialog. Move leaves it for a page tap
 * that places the image, the way a form field is moved. Replace opens a picker
 * for a PNG or JPEG that fills the same box. Delete takes the image off the
 * page at once, with no second question: it is one undoable entry.
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun ImageResizerDialog(resizer: ImageResizer, documentId: Long, actions: ContentEditActions) {
    var width by remember(resizer) { mutableStateOf(resizer.width) }
    var height by remember(resizer) { mutableStateOf(resizer.height) }
    AlertDialog(
        onDismissRequest = actions.onDismissResizer,
        title = { Text(stringResource(R.string.content_image_title)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.content_image_help), style = MaterialTheme.typography.bodySmall)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    PointsField(stringResource(R.string.dialog_width), width, { width = it }, resizer.error != null, Modifier.weight(1f))
                    PointsField(stringResource(R.string.dialog_height), height, { height = it }, resizer.error != null, Modifier.weight(1f))
                }
                resizer.error?.let { error -> Text(error.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onResize(documentId, width, height) }) { Text(stringResource(R.string.dialog_resize)) } },
        dismissButton = {
            // Four actions beside Resize do not fit one row on a phone.
            FlowRow(horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { actions.onDelete(documentId) }) { Text(stringResource(R.string.dialog_delete)) }
                TextButton(onClick = { actions.onReplace(documentId) }) { Text(stringResource(R.string.content_image_replace)) }
                TextButton(onClick = { actions.onMove(documentId) }) { Text(stringResource(R.string.dialog_move)) }
                TextButton(onClick = actions.onDismissResizer) { Text(stringResource(R.string.dialog_cancel)) }
            }
        },
    )
}

/**
 * The insert dialog: a new line and its size, for the spot tapped. Keyed on
 * the inserter, so a refusal reopens with what was typed, like the retype
 * dialog.
 */
@Composable
internal fun TextInserterDialog(inserter: TextInserter, documentId: Long, actions: ContentEditActions) {
    var text by remember(inserter) { mutableStateOf(inserter.text) }
    var size by remember(inserter) { mutableStateOf(inserter.size) }
    AlertDialog(
        onDismissRequest = actions.onDismissInserter,
        title = { Text(stringResource(R.string.content_insert_title, inserter.pageIndex + 1)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(stringResource(R.string.content_insert_help), style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    label = { Text(stringResource(R.string.dialog_text)) },
                    singleLine = true,
                    isError = inserter.error != null,
                )
                PointsField(stringResource(R.string.dialog_size), size, { size = it }, inserter.error != null, Modifier)
                inserter.error?.let { error -> Text(error.resolve(), color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = { Button(onClick = { actions.onInsertText(documentId, text, size) }) { Text(stringResource(R.string.content_insert)) } },
        dismissButton = { TextButton(onClick = actions.onDismissInserter) { Text(stringResource(R.string.dialog_cancel)) } },
    )
}

/** A size in points, typed with a decimal keyboard; shared by the content and annotation dialogs. */
@Composable
internal fun PointsField(label: String, value: String, onValueChange: (String) -> Unit, isError: Boolean, modifier: Modifier, enabled: Boolean = true) {
    OutlinedTextField(
        value = value,
        enabled = enabled,
        onValueChange = onValueChange,
        label = { Text(label) },
        singleLine = true,
        isError = isError,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
        modifier = modifier,
    )
}
