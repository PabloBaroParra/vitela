package dev.vitela.pdf

import android.content.Context
import android.content.Intent
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
    val openPdf = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            val bytes = withContext(Dispatchers.IO) { context.contentResolver.openInputStream(uri)?.use { it.readBytes() } }
            if (bytes == null) {
                viewModel.reportReadFailure()
                return@launch
            }
            runCatching { context.contentResolver.takePersistableUriPermission(uri, Intent.FLAG_GRANT_READ_URI_PERMISSION) }
            val name = uri.lastPathSegment?.substringAfterLast('/') ?: "Document.pdf"
            viewModel.open(name, bytes)
        }
    }
    ViewerScreen(
        state = state,
        onOpen = { openPdf.launch(arrayOf("application/pdf")) },
        onOpenSample = {
            scope.launch {
                val bytes = withContext(Dispatchers.IO) { runCatching { SampleDocument.read(context.assets) }.getOrNull() }
                if (bytes == null) {
                    viewModel.reportReadFailure()
                    return@launch
                }
                viewModel.open(SampleDocument.DISPLAY_NAME, bytes)
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
        onPrint = {
            viewModel.printBytes()?.let { bytes ->
                (context.getSystemService(Context.PRINT_SERVICE) as PrintManager).print(state.title, PdfPrintDocumentAdapter(bytes, state.title), null)
            }
        },
        onPositionChanged = viewModel::onReaderPositionChanged,
        onScrollTargetConsumed = viewModel::consumeScrollTarget,
    )
}
