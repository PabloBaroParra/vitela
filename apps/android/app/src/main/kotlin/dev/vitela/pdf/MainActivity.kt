package dev.vitela.pdf

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Bundle
import android.print.PrintManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.vitela.pdf.core.PdfCoreProvider
import dev.vitela.pdf.document.SafDocuments
import dev.vitela.pdf.document.SafExport
import dev.vitela.pdf.print.PdfPrintDocumentAdapter
import dev.vitela.pdf.sample.SampleDocument
import dev.vitela.pdf.viewer.CERTIFICATE_MIME_TYPES
import dev.vitela.pdf.viewer.ContentEditActions
import dev.vitela.pdf.viewer.FormFieldActions
import dev.vitela.pdf.viewer.IMAGE_REPLACE_CANCELLED
import dev.vitela.pdf.viewer.IMAGE_UNREADABLE
import dev.vitela.pdf.viewer.ImportSource
import dev.vitela.pdf.viewer.OrganizeActions
import dev.vitela.pdf.viewer.SignActions
import dev.vitela.pdf.viewer.ViewerScreen
import dev.vitela.pdf.viewer.ViewerViewModel
import dev.vitela.pdf.viewer.ZoomActions
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent { MaterialTheme { VitelaApp() } }
    }
}

@Composable
private fun VitelaApp(viewModel: ViewerViewModel = viewModel(factory = ViewerViewModelFactory(PdfCoreProvider.create()))) {
    val state by viewModel.state.collectAsState()
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val savePdf = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val snapshot = viewModel.saveSnapshot() ?: return@launch
            val written = withContext(Dispatchers.IO) { SafDocuments.writeCopy(context.contentResolver, uri, snapshot.bytes) }
            if (written) viewModel.confirmSaved(snapshot) else viewModel.reportSaveFailure()
        }
    }
    val chooseStamp = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
            }
            if (bytes == null) viewModel.reportReadFailure() else viewModel.selectImageStamp(bytes)
        }
    }
    val chooseContentImage = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
            }
            if (bytes == null) viewModel.reportReadFailure() else viewModel.armImageInsert(bytes)
        }
    }
    val chooseReplacementImage = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) {
            viewModel.cancelImageReplacement(IMAGE_REPLACE_CANCELLED)
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            val bytes = withContext(Dispatchers.IO) {
                runCatching { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }.getOrNull()
            }
            if (bytes == null) viewModel.cancelImageReplacement(IMAGE_UNREADABLE) else viewModel.replaceImage(bytes)
        }
    }
    val openPdf = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val opened = withContext(Dispatchers.IO) { SafDocuments.open(context.contentResolver, uri) }
            if (opened == null) viewModel.reportReadFailure() else viewModel.open(opened.displayName, opened.bytes, saveTarget = opened.saveTarget)
        }
    }
    val addPdfs = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        if (uris.isEmpty()) return@rememberLauncherForActivityResult
        scope.launch {
            val read = withContext(Dispatchers.IO) { uris.map { SafDocuments.read(context.contentResolver, it) } }
            // All or nothing: adding the readable half of a pick would leave the user to work out which half.
            if (read.any { it == null }) viewModel.reportReadFailure()
            else viewModel.importPdfs(read.filterNotNull().map { ImportSource(it.displayName, it.bytes) })
        }
    }
    val chooseExportFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { treeUri ->
        if (treeUri == null) {
            viewModel.cancelImageExport()
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            viewModel.exportImages { fileName, mimeType, bytes -> SafExport.writeFile(context.contentResolver, treeUri, fileName, mimeType, bytes) }
        }
    }
    val saveExtract = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri ->
        if (uri == null) {
            viewModel.cancelPageExtract()
            return@rememberLauncherForActivityResult
        }
        scope.launch { viewModel.extractPages { bytes -> SafDocuments.writeCopy(context.contentResolver, uri, bytes) } }
    }
    val chooseSplitFolder = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocumentTree()) { treeUri ->
        if (treeUri == null) {
            viewModel.cancelPageSplit()
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            viewModel.splitPages { fileName, bytes -> SafExport.writeFile(context.contentResolver, treeUri, fileName, "application/pdf", bytes) }
        }
    }
    val saveCompressed = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri ->
        if (uri == null) {
            viewModel.cancelCompress()
            return@rememberLauncherForActivityResult
        }
        scope.launch { viewModel.writeCompressed { bytes -> SafDocuments.writeCopy(context.contentResolver, uri, bytes) } }
    }
    val saveProtected = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri ->
        if (uri == null) {
            viewModel.cancelProtect()
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            val created = withContext(Dispatchers.IO) { SafDocuments.created(context.contentResolver, uri) }
            viewModel.writeProtected(created.displayName, created.saveTarget) { bytes -> SafDocuments.writeCopy(context.contentResolver, uri, bytes) }
        }
    }
    val chooseCertificate = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val read = withContext(Dispatchers.IO) { SafDocuments.read(context.contentResolver, uri) }
            if (read == null) viewModel.reportReadFailure() else viewModel.chooseSigningCertificate(read.displayName, read.bytes)
        }
    }
    val saveSigned = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/pdf")) { uri ->
        if (uri == null) {
            viewModel.cancelSign()
            return@rememberLauncherForActivityResult
        }
        scope.launch {
            val created = withContext(Dispatchers.IO) { SafDocuments.created(context.contentResolver, uri) }
            viewModel.writeSigned(created.displayName, created.saveTarget) { bytes -> SafDocuments.writeCopy(context.contentResolver, uri, bytes) }
        }
    }
    ViewerScreen(
        state = state,
        onOpen = { openPdf.launch(arrayOf("application/pdf")) },
        onOpenSample = { assetName, displayName ->
            scope.launch {
                val bytes = withContext(Dispatchers.IO) { runCatching { SampleDocument.read(context.assets, assetName) }.getOrNull() }
                if (bytes == null) {
                    viewModel.reportReadFailure()
                    return@launch
                }
                viewModel.open(displayName, bytes)
            }
        },
        onPrevious = { viewModel.navigate(-1) },
        onNext = { viewModel.navigate(1) },
        onGoToPage = viewModel::goToPage,
        zoom = remember(viewModel) {
            ZoomActions(
                onZoomIn = viewModel::zoomIn,
                onZoomOut = viewModel::zoomOut,
                onFitWidth = viewModel::fitWidth,
                onFitPage = viewModel::fitPage,
                onPinch = viewModel::setZoom,
            )
        },
        onSearch = viewModel::search,
        onPreviousMatch = { viewModel.stepSearch(-1) },
        onNextMatch = { viewModel.stepSearch(1) },
        onPassword = viewModel::retryPassword,
        onPasswordCancel = viewModel::cancelPassword,
        onPrint = {
            scope.launch {
                viewModel.printBytes()?.let { bytes ->
                    (context.getSystemService(Context.PRINT_SERVICE) as PrintManager).print(state.title, PdfPrintDocumentAdapter(bytes, state.title), null)
                }
            }
        },
        onSave = {
            scope.launch {
                val save = viewModel.inPlaceSave() ?: return@launch
                val written = withContext(Dispatchers.IO) { SafDocuments.writeBack(context.contentResolver, save.target, save.snapshot.bytes) }
                if (written) viewModel.confirmSaved(save.snapshot) else viewModel.reportInPlaceSaveFailure(save)
            }
        },
        onSaveCopy = { savePdf.launch(state.title.ifBlank { "Document.pdf" }) },
        onChooseStamp = { chooseStamp.launch("image/*") },
        onReplacementConfirmed = viewModel::confirmReplacement,
        onReplacementCancelled = viewModel::cancelReplacement,
        onPositionChanged = viewModel::onReaderPositionChanged,
        onScrollTargetConsumed = viewModel::consumeScrollTarget,
        onAnnotationTool = viewModel::setAnnotationTool,
        onAnnotationGesture = viewModel::handlePageGesture,
        onAnnotationColor = viewModel::restyleSelected,
        onAnnotationGrow = viewModel::growSelected,
        onAnnotationDelete = viewModel::deleteSelected,
        onAnnotationUndo = viewModel::undoAnnotations,
        onAnnotationRedo = viewModel::redoAnnotations,
        onTextSelectionStart = viewModel::beginTextSelection,
        onTextSelectionMove = viewModel::extendTextSelection,
        onTextSelectionEnd = viewModel::endTextSelection,
        onCopySelection = {
            state.textSelection?.text?.let { text ->
                context.getSystemService(ClipboardManager::class.java).setPrimaryClip(ClipData.newPlainText(state.title, text))
            }
        },
        onOpenMetadata = viewModel::openMetadata,
        onMetadataChange = viewModel::editMetadata,
        onMetadataApply = viewModel::applyMetadata,
        onMetadataDismiss = viewModel::dismissMetadata,
        onOpenImageExport = viewModel::openImageExport,
        onImageExportChange = viewModel::editImageExport,
        // The choices are checked first; only a plan that will be accepted asks for a folder.
        onImageExportConfirm = { scope.launch { if (viewModel.planImageExport()) chooseExportFolder.launch(null) } },
        onImageExportDismiss = viewModel::dismissImageExport,
        onOpenPageExtract = viewModel::openPageExtract,
        onPageExtractChange = viewModel::editPageExtract,
        // Same order as Export images: only an accepted range asks where to write.
        onPageExtractConfirm = { scope.launch { viewModel.planPageExtract()?.let(saveExtract::launch) } },
        onPageExtractDismiss = viewModel::dismissPageExtract,
        onOpenPageSplit = viewModel::openPageSplit,
        onPageSplitChange = viewModel::editPageSplit,
        // Same order as Export images: only accepted cuts ask for a folder.
        onPageSplitConfirm = { scope.launch { if (viewModel.planPageSplit()) chooseSplitFolder.launch(null) } },
        onPageSplitDismiss = viewModel::dismissPageSplit,
        onOpenCompress = viewModel::openCompress,
        onCompressSelect = viewModel::selectCompressPreset,
        // The destination comes last: only a copy that came out smaller asks where to go.
        onCompressConfirm = { scope.launch { viewModel.compress()?.let(saveCompressed::launch) } },
        onCompressDismiss = viewModel::dismissCompress,
        onOpenProtect = viewModel::openProtect,
        // Passwords first, then where to write; the protected file is reopened once written.
        onProtectConfirm = { openPassword, permissionsPassword -> viewModel.confirmProtect(openPassword, permissionsPassword)?.let(saveProtected::launch) },
        onProtectDismiss = viewModel::dismissProtect,
        sign = remember(viewModel) {
            SignActions(
                onOpen = viewModel::openSign,
                onChooseCertificate = { chooseCertificate.launch(CERTIFICATE_MIME_TYPES) },
                onUnlock = viewModel::unlockSigningCertificate,
                onSelectIdentity = viewModel::selectSigningIdentity,
                // Who signs first, then where to write; the signed file is reopened once written.
                onSign = { viewModel.confirmSign()?.let(saveSigned::launch) },
                onDismiss = viewModel::dismissSign,
            )
        },
        organize = remember(viewModel) {
            OrganizeActions(
                onToggle = { if (viewModel.state.value.organize == null) viewModel.openOrganize() else viewModel.closeOrganize() },
                onMove = viewModel::organizeMove,
                onRotate = viewModel::organizeRotate,
                onDelete = viewModel::organizeDelete,
                onInsertBlank = viewModel::organizeInsertBlank,
                onThumbnail = viewModel::organizeThumbnail,
                onAddPdfs = { addPdfs.launch(arrayOf("application/pdf")) },
                onImportPassword = viewModel::retryImportPassword,
                onImportPasswordCancel = viewModel::cancelImportPassword,
                onImportWarningsDismiss = viewModel::dismissImportWarnings,
                onShow = viewModel::organizeShow,
                onMoveBlock = viewModel::organizeMoveBlock,
                onRotateBlock = viewModel::organizeRotateBlock,
                onDeleteBlock = viewModel::organizeDeleteBlock,
            )
        },
        formFields = remember(viewModel) {
            FormFieldActions(
                onToggle = { if (viewModel.state.value.formFields == null) viewModel.openFormFields() else viewModel.closeFormFields() },
                onFill = viewModel::fillFormField,
                onArm = viewModel::armFormField,
                onPageTap = viewModel::tapFormField,
                onResize = viewModel::resizeFormField,
            )
        },
        contentEdit = remember(viewModel) {
            ContentEditActions(
                onToggle = { if (viewModel.state.value.contentEdit == null) viewModel.openContentEdit() else viewModel.closeContentEdit() },
                onPageShown = viewModel::contentPageShown,
                onTap = viewModel::tapContent,
                onRetype = viewModel::retypeTextRun,
                onDeleteText = viewModel::deleteTextRun,
                onMoveText = viewModel::armTextMove,
                onDismiss = viewModel::dismissTextRunEditor,
                onResize = viewModel::resizeImage,
                onDismissResizer = viewModel::dismissImageResizer,
                onMove = viewModel::armImageMove,
                onCancelMove = viewModel::cancelContentMove,
                onDelete = viewModel::deleteImage,
                // The core decodes PNG and JPEG; anything else is refused when it lands.
                onReplace = { documentId -> scope.launch { if (viewModel.prepareImageReplacement(documentId)) chooseReplacementImage.launch("image/*") } },
                onAddText = viewModel::armTextInsert,
                // The core decodes PNG and JPEG; anything else is refused when placed.
                onAddImage = { chooseContentImage.launch("image/*") },
                onCancelInsert = viewModel::cancelInsert,
                onInsertText = viewModel::insertText,
                onDismissInserter = viewModel::dismissTextInserter,
            )
        },
    )
}
