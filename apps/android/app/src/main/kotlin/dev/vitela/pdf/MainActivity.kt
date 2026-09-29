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
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalContext
import androidx.lifecycle.viewmodel.compose.viewModel
import dev.vitela.pdf.core.PdfCoreProvider
import dev.vitela.pdf.document.SafDocuments
import dev.vitela.pdf.print.PdfPrintDocumentAdapter
import dev.vitela.pdf.sample.SampleDocument
import dev.vitela.pdf.viewer.ViewerScreen
import dev.vitela.pdf.viewer.ViewerViewModel
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
    val openPdf = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val opened = withContext(Dispatchers.IO) { SafDocuments.open(context.contentResolver, uri) }
            if (opened == null) viewModel.reportReadFailure() else viewModel.open(opened.displayName, opened.bytes, saveTarget = opened.saveTarget)
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
        onZoomOut = viewModel::zoomOut,
        onZoomIn = viewModel::zoomIn,
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
    )
}
