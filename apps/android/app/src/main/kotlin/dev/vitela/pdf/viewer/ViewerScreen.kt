package dev.vitela.pdf.viewer

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.Surface
import androidx.compose.material3.FilterChip
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.home.HomeScreen
import dev.vitela.pdf.home.RecentActions
import dev.vitela.pdf.home.RecentDocument
import dev.vitela.pdf.ui.theme.Vitela

/**
 * The app's one screen: Home while no document is open, the reader once one
 * is. The continuous [PageList] takes the remaining height. Tablets put the
 * mode tabs and tools beside it and can show two pages in each scrolling row.
 * Nothing around the list scrolls — a lazy list inside a scrolling parent is
 * measured with an infinite height and would rasterize every page at once.
 */
@Composable
internal fun ViewerScreen(
    state: ViewerState,
    onOpen: () -> Unit,
    onClose: () -> Unit,
    onCloseConfirmed: () -> Unit,
    onCloseCancelled: () -> Unit,
    onOpenTool: (DocumentStartTool) -> Unit,
    onOpenSample: (assetName: String, displayName: String) -> Unit,
    recents: List<RecentDocument>,
    recentActions: RecentActions,
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
    onNoteRead: () -> Unit,
    onNoteReadingClose: () -> Unit,
    onAnnotationGesture: (Int, dev.vitela.pdf.core.AnnotationPoint, dev.vitela.pdf.core.AnnotationPoint, List<dev.vitela.pdf.core.AnnotationPoint>, Double) -> Unit,
    onAnnotationColor: (dev.vitela.pdf.core.AnnotationColor) -> Unit,
    onAnnotationGrow: () -> Unit,
    onAnnotationResize: () -> Unit,
    onAnnotationResizeConfirm: (Long, String, String) -> Unit,
    onAnnotationResizeCancel: () -> Unit,
    onAnnotationPosition: () -> Unit,
    onAnnotationPositionConfirm: (Long, String, String) -> Unit,
    onAnnotationPositionCancel: () -> Unit,
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
    drawnSignature: DrawnSignatureActions,
    organize: OrganizeActions,
    formFields: FormFieldActions,
    contentEdit: ContentEditActions,
    fileDrop: FileDropActions,
) {
    // Files dragged in from another app: whatever missed every page lands here, Home included.
    Box(Modifier.fillMaxSize().fileDropTarget(fileDrop.onScreen)) {
        if (showsHome(state)) {
            HomeScreen(canOpen = state.canOpen && !state.isLoading && !state.needsPassword, isLoading = state.isLoading, status = state.status, onOpen = onOpen, onOpenTool = onOpenTool, onOpenSample = onOpenSample, recents = recents, recentActions = recentActions)
        } else {
            var mode by rememberSaveable(state.documentId) { mutableStateOf(initialReaderMode(state.startTool)) }
            var searchOpen by rememberSaveable { mutableStateOf(false) }
            var pageListOpen by remember { mutableStateOf(false) }
            var spread by rememberSaveable(state.documentId) { mutableStateOf(true) }
            val switchMode: (ReaderMode) -> Unit = { to ->
                val exit = modeExit(mode, to, state)
                if (exit.disarmTool) onAnnotationTool(AnnotationTool.Pointer)
                if (exit.closeContentEdit) contentEdit.onToggle()
                if (exit.closeFormFields) formFields.onToggle()
                mode = to
            }
            val selected = state.annotations.lastOrNull { it.id == state.selectedAnnotationId }
            BackHandler {
                if (documentCloseEnabled(state)) when {
                    searchOpen -> searchOpen = false
                    state.organize != null -> organize.onToggle()
                    state.contentEdit != null -> contentEdit.onToggle()
                    state.formFields != null -> formFields.onToggle()
                    else -> onClose()
                }
            }
            val controls = annotationControls(state.annotationEditingAllowed, selected, state.canUndoAnnotations, state.canRedoAnnotations)
                .let { if (state.organize != null) it.whileOrganizing() else it }
            BoxWithConstraints(modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background)) {
                val layout = readerLayout(maxWidth.value)
                val columns = if (layout.canShowSpread && spread) 2 else 1
                val tools: @Composable () -> Unit = {
                    when (mode) {
                        ReaderMode.Read -> QuickTools(state, controls, QuickToolActions(
                            onEditText = {
                                switchMode(ReaderMode.Edit)
                                if (state.contentEdit == null) contentEdit.onToggle()
                            },
                            onHighlight = {
                                switchMode(ReaderMode.Edit)
                                onAnnotationTool(AnnotationTool.Highlight)
                            },
                            onSign = {
                                switchMode(ReaderMode.Sign)
                                sign.onOpen()
                            },
                            onOrganize = organize.onToggle,
                            onCompress = onOpenCompress,
                        ), sidePanel = layout.sideTools)
                        ReaderMode.Edit -> EditTools(state, controls, EditToolActions(
                            onTool = onAnnotationTool, onChooseStamp = onChooseStamp, onPasteStamp = onPasteStamp,
                            onUndo = onAnnotationUndo, onRedo = onAnnotationRedo, onOrganize = organize.onToggle, contentEdit = contentEdit,
                        ), sidePanel = layout.sideTools)
                        ReaderMode.Sign -> SignTools(state, controls, onFormFields = formFields.onToggle, onDrawSignature = drawnSignature.onOpen, onSign = sign.onOpen, sidePanel = layout.sideTools)
                    }
                }
                Column(Modifier.fillMaxSize()) {
                    ReaderTopBar(
                        state = state,
                        searchOpen = searchOpen,
                        onSearchToggle = { searchOpen = !searchOpen },
                        onSave = if (state.saveTarget != null) onSave else onSaveCopy,
                        menu = DocumentMenuActions(
                            onOpen = onOpen, onSave = onSave, onSaveCopy = onSaveCopy, onPrint = onPrint, onProperties = onOpenMetadata,
                            onExportImages = onOpenImageExport, onExtractPages = onOpenPageExtract, onSplit = onOpenPageSplit,
                            onCompress = onOpenCompress, onProtect = onOpenProtect, onAnnotationStep = onAnnotationStep,
                            onClose = onClose,
                        ),
                        onOrganizeDone = organize.onToggle,
                        onClose = onClose,
                    )
                    if (state.isLoading) LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                    Row(Modifier.fillMaxWidth().weight(1f).then(if (layout.sideTools) Modifier.navigationBarsPadding() else Modifier)) {
                        Column(Modifier.weight(1f).fillMaxHeight()) {
                            if (modeBarVisible(state) && !layout.sideTools) ModeTabs(mode, switchMode)
                            if (layout.canShowSpread && state.organize == null) {
                                Row(Modifier.padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                                    Text("Page layout", style = MaterialTheme.typography.labelLarge)
                                    FilterChip(selected = !spread, onClick = { spread = false }, label = { Text("One page") })
                                    FilterChip(selected = spread, onClick = { spread = true }, label = { Text("Two pages") })
                                }
                            }
                            if (searchOpen && state.organize == null) {
                                SearchRow(state, onSearch, onPreviousMatch, onNextMatch, onClose = { searchOpen = false })
                            }
                            ContextChips(
                                state, controls,
                                ContextActions(
                                    onCopy = onCopySelection, onReadNote = onNoteRead, onGrow = onAnnotationGrow, onResize = onAnnotationResize,
                                    onPosition = onAnnotationPosition, onDelete = onAnnotationDelete, onColor = onAnnotationColor,
                                    onCancelMove = contentEdit.onCancelMove, onCancelInsert = contentEdit.onCancelInsert,
                                ),
                            )
                            Box(modifier = Modifier.fillMaxWidth().weight(1f).background(Vitela.colors.canvas)) {
                                val organizeState = state.organize
                                if (organizeState != null) {
                                    OrganizeGrid(state, organizeState, organize, modifier = Modifier.fillMaxSize())
                                } else {
                                    PageList(
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
                                    onFileDrop = fileDrop.onPage,
                                        modifier = Modifier.fillMaxSize().padding(if (layout.sideTools) 20.dp else 0.dp),
                                        columns = columns,
                                    )
                                    Row(
                                        modifier = Modifier.align(Alignment.BottomCenter).padding(bottom = 12.dp),
                                        horizontalArrangement = Arrangement.spacedBy(8.dp),
                                    ) {
                                        PageStepper(state, onPrevious, onNext, onPageList = { pageListOpen = true })
                                        ZoomPill(state, zoom)
                                    }
                                }
                            }
                            state.formFields?.let { panel ->
                                FormFieldsPanel(panel, state.documentId, formFields, modifier = Modifier.fillMaxWidth().heightIn(max = 280.dp))
                            }
                            Text(
                                state.status,
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                maxLines = 2,
                                modifier = Modifier.padding(horizontal = 16.dp, vertical = 6.dp),
                            )
                            if (state.organize != null) {
                                Spacer(Modifier.navigationBarsPadding())
                            } else if (!layout.sideTools) tools()
                        }
                        if (layout.sideTools && state.organize == null) {
                            Surface(modifier = Modifier.width(280.dp).fillMaxHeight(), color = MaterialTheme.colorScheme.surface, shadowElevation = 2.dp) {
                                Column {
                                    if (modeBarVisible(state)) ModeTabs(mode, switchMode)
                                    tools()
                                }
                            }
                        }
                    }
                }
            }
            // Closed, not just hidden, once navigation is disabled: a load or the
            // grid would otherwise reopen it over pages the reader no longer shows.
            val navigationEnabled = pageNavigationEnabled(state)
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
        }
    }
    ViewerDialogs(
        state,
        DialogActions(
            onPassword = onPassword, onPasswordCancel = onPasswordCancel,
            onReplacementConfirmed = onReplacementConfirmed, onReplacementCancelled = onReplacementCancelled,
            onCloseConfirmed = onCloseConfirmed, onCloseCancelled = onCloseCancelled,
            onNoteAdd = onNoteAdd, onNoteCancel = onNoteCancel, onNoteReadingClose = onNoteReadingClose,
            onAnnotationResizeConfirm = onAnnotationResizeConfirm, onAnnotationResizeCancel = onAnnotationResizeCancel,
            onAnnotationPositionConfirm = onAnnotationPositionConfirm, onAnnotationPositionCancel = onAnnotationPositionCancel,
            onMetadataChange = onMetadataChange, onMetadataApply = onMetadataApply, onMetadataDismiss = onMetadataDismiss,
            onImageExportChange = onImageExportChange, onImageExportConfirm = onImageExportConfirm, onImageExportDismiss = onImageExportDismiss,
            onPageExtractChange = onPageExtractChange, onPageExtractConfirm = onPageExtractConfirm, onPageExtractDismiss = onPageExtractDismiss,
            onPageSplitChange = onPageSplitChange, onPageSplitConfirm = onPageSplitConfirm, onPageSplitDismiss = onPageSplitDismiss,
            onCompressSelect = onCompressSelect, onCompressConfirm = onCompressConfirm, onCompressDismiss = onCompressDismiss,
            onProtectConfirm = onProtectConfirm, onProtectDismiss = onProtectDismiss,
            sign = sign, drawnSignature = drawnSignature, contentEdit = contentEdit,
        ),
    )
}
