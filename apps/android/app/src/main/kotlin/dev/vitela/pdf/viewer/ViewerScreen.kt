package dev.vitela.pdf.viewer

import androidx.compose.foundation.Image
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.R
import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.sample.SampleDocument

/**
 * Reader chrome around the continuous [PageList]. The controls are a fixed
 * header and the list takes the remaining height — the whole screen must not
 * scroll, because a lazy list inside a scrolling parent is measured with an
 * infinite height and would rasterize every page at once.
 */
@Composable
internal fun ViewerScreen(
    state: ViewerState,
    onOpen: () -> Unit,
    onOpenSample: (assetName: String, displayName: String) -> Unit,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onGoToPage: (Int) -> Unit,
    zoom: ZoomActions,
    onSearch: (String) -> Unit,
    onPreviousMatch: () -> Unit,
    onNextMatch: () -> Unit,
    onPassword: (String) -> Unit,
    onPasswordCancel: () -> Unit,
    onPrint: () -> Unit,
    onSave: () -> Unit,
    onSaveCopy: () -> Unit,
    onChooseStamp: () -> Unit,
    onPasteStamp: () -> Unit,
    onReplacementConfirmed: () -> Unit,
    onReplacementCancelled: () -> Unit,
    onPositionChanged: (ReaderPosition) -> Unit,
    onScrollTargetConsumed: () -> Unit,
    onAnnotationRevealConsumed: () -> Unit,
    onAnnotationStep: (forward: Boolean) -> Unit,
    onAnnotationTool: (AnnotationTool) -> Unit,
    onNoteAdd: (Long, String) -> Unit,
    onNoteCancel: () -> Unit,
    onAnnotationGesture: (Int, dev.vitela.pdf.core.AnnotationPoint, dev.vitela.pdf.core.AnnotationPoint, List<dev.vitela.pdf.core.AnnotationPoint>, Double) -> Unit,
    onAnnotationColor: (dev.vitela.pdf.core.AnnotationColor) -> Unit,
    onAnnotationGrow: () -> Unit,
    onAnnotationDelete: () -> Unit,
    onAnnotationUndo: () -> Unit,
    onAnnotationRedo: () -> Unit,
    onTextSelectionStart: (Int, dev.vitela.pdf.core.AnnotationPoint) -> Unit,
    onTextSelectionMove: (dev.vitela.pdf.core.AnnotationPoint) -> Unit,
    onTextSelectionEnd: () -> Unit,
    onCopySelection: () -> Unit,
    onOpenMetadata: () -> Unit,
    onMetadataChange: (dev.vitela.pdf.core.DocumentInfo) -> Unit,
    onMetadataApply: () -> Unit,
    onMetadataDismiss: () -> Unit,
    onOpenImageExport: () -> Unit,
    onImageExportChange: (ImageExportDraft) -> Unit,
    onImageExportConfirm: () -> Unit,
    onImageExportDismiss: () -> Unit,
    onOpenPageExtract: () -> Unit,
    onPageExtractChange: (String) -> Unit,
    onPageExtractConfirm: () -> Unit,
    onPageExtractDismiss: () -> Unit,
    onOpenPageSplit: () -> Unit,
    onPageSplitChange: (String) -> Unit,
    onPageSplitConfirm: () -> Unit,
    onPageSplitDismiss: () -> Unit,
    onOpenCompress: () -> Unit,
    onCompressSelect: (CompressPreset) -> Unit,
    onCompressConfirm: () -> Unit,
    onCompressDismiss: () -> Unit,
    onOpenProtect: () -> Unit,
    onProtectConfirm: (openPassword: String, permissionsPassword: String) -> Unit,
    onProtectDismiss: () -> Unit,
    sign: SignActions,
    organize: OrganizeActions,
    formFields: FormFieldActions,
    contentEdit: ContentEditActions,
) {
    var query by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var sampleMenuExpanded by remember { mutableStateOf(false) }
    var pageListOpen by remember { mutableStateOf(false) }
    val zoomPercentage = (state.zoomFactor * 100).toInt()
    Column(
        modifier = Modifier.fillMaxSize().padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(state.title, style = MaterialTheme.typography.headlineSmall)
        Row(
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            modifier = Modifier.horizontalScroll(rememberScrollState()),
        ) {
            Button(onClick = onOpen, enabled = state.canOpen) { Text("Open PDF") }
            Box {
                Button(onClick = { sampleMenuExpanded = true }, enabled = state.canOpen) { Text("Open sample") }
                DropdownMenu(expanded = sampleMenuExpanded, onDismissRequest = { sampleMenuExpanded = false }) {
                    DropdownMenuItem(
                        text = { Text("Vitela sample") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.ASSET_NAME, SampleDocument.DISPLAY_NAME)
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("AES-128 sample (user-aes-pass)") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.AES128_ASSET_NAME, SampleDocument.AES128_DISPLAY_NAME)
                        },
                    )
                    DropdownMenuItem(
                        text = { Text("RC4-128 sample (user-rc4-pass)") },
                        onClick = {
                            sampleMenuExpanded = false
                            onOpenSample(SampleDocument.RC4_128_ASSET_NAME, SampleDocument.RC4_128_DISPLAY_NAME)
                        },
                    )
                }
            }
            Button(onClick = onPrint, enabled = state.canPrint) { Text("Print") }
            Button(onClick = onSave, enabled = state.isDirty && state.saveTarget != null) { Text("Save") }
            Button(onClick = onSaveCopy, enabled = state.isDirty) { Text("Save copy") }
            Button(onClick = onOpenMetadata, enabled = state.pageCount > 0) { Text("Properties") }
            Button(onClick = onOpenImageExport, enabled = state.pageCount > 0 && !state.imageExportRunning) { Text("Export images") }
            Button(onClick = onOpenPageExtract, enabled = state.pageCount > 0) { Text("Extract pages") }
            Button(onClick = onOpenPageSplit, enabled = state.pageCount > 1 && !state.pageSplitRunning) { Text("Split") }
            Button(onClick = onOpenCompress, enabled = state.pageCount > 0 && !state.compressRunning) { Text("Compress") }
            Button(onClick = onOpenProtect, enabled = state.pageCount > 0 && !state.protectRunning) { Text("Protect") }
            Button(onClick = sign.onOpen, enabled = state.pageCount > 0 && !state.signRunning) { Text("Sign") }
            Button(onClick = organize.onToggle, enabled = state.pageCount > 0) { Text(if (state.organize != null) "Done" else "Organize") }
            // Not over the grid: it hides the pages a fill redraws.
            Button(onClick = formFields.onToggle, enabled = state.pageCount > 0 && state.organize == null) { Text("Form fields") }
            // Same reason: the grid hides the pages a retype or an image edit redraws.
            Button(onClick = contentEdit.onToggle, enabled = state.pageCount > 0 && state.organize == null) { Text(if (state.contentEdit != null) "Done editing" else "Edit content") }
            if (state.contentEdit?.moving != null) TextButton(onClick = contentEdit.onCancelMove) { Text("Cancel move") }
            if (state.contentEdit != null) {
                TextButton(onClick = contentEdit.onAddText) { Text("Add text") }
                TextButton(onClick = contentEdit.onAddImage) { Text("Add image") }
            }
            if (state.contentEdit?.adding != null) TextButton(onClick = contentEdit.onCancelInsert) { Text("Cancel insert") }
        }
        val navigationEnabled = pageNavigationEnabled(state)
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            Button(onClick = onPrevious, enabled = state.pageIndex > 0) { Text("Previous") }
            Button(onClick = onNext, enabled = state.pageIndex + 1 < state.pageCount) { Text("Next") }
            if (state.pageCount == 0) Text("No pages")
            else TextButton(onClick = { pageListOpen = true }, enabled = navigationEnabled) {
                Text("Page ${state.pageIndex + 1} of ${state.pageCount}")
            }
        }
        // Closed, not just hidden, once navigation is disabled: a load or the
        // grid would otherwise reopen it over pages the reader no longer shows.
        LaunchedEffect(navigationEnabled) { if (!navigationEnabled) pageListOpen = false }
        if (pageListOpen && navigationEnabled) {
            PageNavigationDialog(
                pageCount = state.pageCount,
                currentPage = state.pageIndex,
                onSelect = { pageIndex ->
                    pageListOpen = false
                    onGoToPage(pageIndex)
                },
                onDismiss = { pageListOpen = false },
            )
        }
        val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
        val annotationControls = annotationControls(state.annotationEditingAllowed, selected, state.canUndoAnnotations, state.canRedoAnnotations)
            .let { if (state.organize != null) it.whileOrganizing() else it }
        Row(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.horizontalScroll(rememberScrollState()),
        ) {
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Pointer) }, enabled = annotationControls.canCreate) { Text("Select") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Highlight) }, enabled = annotationControls.canCreate) { Text("Highlight") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Underline) }, enabled = annotationControls.canCreate) { Text("Underline") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Strikeout) }, enabled = annotationControls.canCreate) { Text("Strikeout") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Ink) }, enabled = annotationControls.canCreate) { Text("Ink") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.Shape) }, enabled = annotationControls.canCreate) { Text("Shape") }
            TextButton(onClick = { onAnnotationTool(AnnotationTool.TextNote) }, enabled = annotationControls.canCreate) { Text("Note") }
            TextButton(onClick = onChooseStamp, enabled = annotationControls.canCreate) { Text("Stamp") }
            TextButton(onClick = onPasteStamp, enabled = annotationControls.canCreate) { Text("Paste") }
        }
        Row(
            horizontalArrangement = Arrangement.spacedBy(6.dp),
            verticalAlignment = Alignment.CenterVertically,
            modifier = Modifier.horizontalScroll(rememberScrollState()),
        ) {
            // Not gated on editing: visiting an annotation changes nothing.
            val annotationNavigation = annotationNavigationEnabled(state)
            TextButton(onClick = { onAnnotationStep(false) }, enabled = annotationNavigation) { Text("Previous annotation") }
            TextButton(onClick = { onAnnotationStep(true) }, enabled = annotationNavigation) { Text("Next annotation") }
            TextButton(onClick = onCopySelection, enabled = state.textSelection != null) { Text("Copy") }
            TextButton(onClick = onAnnotationGrow, enabled = annotationControls.canGrow) { Text("Grow") }
            TextButton(onClick = onAnnotationDelete, enabled = selected != null && state.annotationEditingAllowed) { Text("Delete") }
            TextButton(onClick = { onAnnotationColor(dev.vitela.pdf.core.AnnotationColor(220, 40, 40)) }, enabled = annotationControls.canRestyle) { Text("Red") }
            TextButton(onClick = { onAnnotationColor(DEFAULT_ANNOTATION_COLOR) }, enabled = annotationControls.canRestyle) { Text("Gold") }
            TextButton(onClick = onAnnotationUndo, enabled = annotationControls.canUndo || state.canUndoAnnotations) { Text("Undo") }
            TextButton(onClick = onAnnotationRedo, enabled = annotationControls.canRedo || state.canRedoAnnotations) { Text("Redo") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            TextButton(onClick = zoom.onZoomOut, enabled = state.pageCount > 0 && state.zoomFactor > MIN_ZOOM_FACTOR) { Text("Zoom out") }
            Text("$zoomPercentage%", modifier = Modifier.semantics { contentDescription = "Zoom level: $zoomPercentage%" })
            TextButton(onClick = zoom.onZoomIn, enabled = state.pageCount > 0 && state.zoomFactor < MAX_ZOOM_FACTOR) { Text("Zoom in") }
            TextButton(onClick = zoom.onFitWidth, enabled = state.pageCount > 0 && state.zoomFactor != DEFAULT_ZOOM_FACTOR) { Text("Fit width") }
            TextButton(onClick = zoom.onFitPage, enabled = state.pageCount > 0) { Text("Fit page") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            OutlinedTextField(value = query, onValueChange = { query = it }, label = { Text("Find text") }, modifier = Modifier.weight(1f))
            Button(onClick = { onSearch(query) }, enabled = state.pageCount > 0) { Text("Find") }
        }
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = onPreviousMatch, enabled = state.searchHits.isNotEmpty()) { Text("Previous match") }
            TextButton(onClick = onNextMatch, enabled = state.searchHits.isNotEmpty()) { Text("Next match") }
        }
        if (state.isLoading) CircularProgressIndicator(modifier = Modifier.size(28.dp))
        Box(modifier = Modifier.fillMaxWidth().weight(1f)) {
            val organizeState = state.organize
            if (organizeState != null) {
                OrganizeGrid(state, organizeState, organize, modifier = Modifier.fillMaxSize())
            } else PageList(
                state = state,
                onPositionChanged = onPositionChanged,
                onScrollTargetConsumed = onScrollTargetConsumed,
                onAnnotationRevealConsumed = onAnnotationRevealConsumed,
                onAnnotationGesture = onAnnotationGesture,
                textSelection = remember(onTextSelectionStart, onTextSelectionMove, onTextSelectionEnd) {
                    TextSelectionGestures(onTextSelectionStart, onTextSelectionMove, onTextSelectionEnd)
                },
                contentEdit = contentEdit,
                onFormFieldTap = formFields.onPageTap,
                onPinch = zoom.onPinch,
                modifier = Modifier.fillMaxSize(),
            )
            // Mirrors the WinUI empty state and the GTK4 shell's overlay mark:
            // shown while the page area has nothing to display, hidden once a
            // document with pages is open.
            if (state.pageCount == 0) {
                Image(
                    painter = painterResource(R.drawable.ic_app_mark),
                    contentDescription = null,
                    modifier = Modifier.align(Alignment.Center).size(96.dp),
                )
            }
        }
        state.formFields?.let { panel ->
            FormFieldsPanel(panel, state.documentId, formFields, modifier = Modifier.fillMaxWidth().heightIn(max = 280.dp))
        }
        Text(state.status, style = MaterialTheme.typography.bodyMedium)
    }
    if (state.needsPassword) {
        var passwordVisible by remember { mutableStateOf(false) }
        val cancel = { onPasswordCancel(); password = "" }
        AlertDialog(
            onDismissRequest = cancel,
            title = { Text("Password required") },
            text = {
                Column {
                    state.passwordMessage?.let { Text(it) }
                    OutlinedTextField(
                        value = password,
                        onValueChange = { password = it },
                        label = { Text("Password") },
                        visualTransformation = if (passwordVisible) VisualTransformation.None else PasswordVisualTransformation(),
                        trailingIcon = {
                            TextButton(onClick = { passwordVisible = !passwordVisible }) {
                                Text(if (passwordVisible) "Hide" else "Show")
                            }
                        },
                    )
                }
            },
            confirmButton = { Button(onClick = { onPassword(password); password = "" }) { Text("Open") } },
            dismissButton = { TextButton(onClick = cancel) { Text("Cancel") } },
        )
    }
    state.contentEdit?.editor?.let { editor ->
        TextRunEditorDialog(editor, state.documentId, contentEdit)
    }
    state.contentEdit?.resizer?.let { resizer ->
        ImageResizerDialog(resizer, state.documentId, contentEdit)
    }
    state.contentEdit?.inserter?.let { inserter ->
        TextInserterDialog(inserter, state.documentId, contentEdit)
    }
    state.notePlacement?.let { placement ->
        NoteDialog(placement, state.documentId, onAdd = onNoteAdd, onCancel = onNoteCancel)
    }
    state.metadataEditor?.let { editor ->
        MetadataDialog(editor, onChange = onMetadataChange, onApply = onMetadataApply, onDismiss = onMetadataDismiss)
    }
    state.imageExport?.let { editor ->
        ImageExportDialog(editor, onChange = onImageExportChange, onExport = onImageExportConfirm, onDismiss = onImageExportDismiss)
    }
    state.pageExtract?.let { editor ->
        PageExtractDialog(editor, onChange = onPageExtractChange, onExtract = onPageExtractConfirm, onDismiss = onPageExtractDismiss)
    }
    state.pageSplit?.let { editor ->
        PageSplitDialog(editor, state.pageCount, onChange = onPageSplitChange, onSplit = onPageSplitConfirm, onDismiss = onPageSplitDismiss)
    }
    state.compress?.let { editor ->
        CompressDialog(editor, onSelect = onCompressSelect, onCompress = onCompressConfirm, onDismiss = onCompressDismiss)
    }
    state.protect?.let { editor ->
        ProtectDialog(editor, onProtect = onProtectConfirm, onDismiss = onProtectDismiss)
    }
    state.sign?.let { editor -> SignDialog(editor, sign) }
    state.pendingReplacementTitle?.let { title ->
        AlertDialog(
            onDismissRequest = onReplacementCancelled,
            title = { Text("Discard unsaved changes?") },
            text = { Text("Open $title and discard the current unsaved changes?") },
            confirmButton = { Button(onClick = onReplacementConfirmed) { Text("Discard and open") } },
            dismissButton = { TextButton(onClick = onReplacementCancelled) { Text("Keep editing") } },
        )
    }
}
