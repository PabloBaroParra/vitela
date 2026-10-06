package dev.vitela.pdf

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.viewer.FileSignatureStore
import dev.vitela.pdf.viewer.SIGNATURE_FILE_NAME
import dev.vitela.pdf.viewer.ViewerViewModel
import java.io.File

/** [filesDir] is the app's private files directory, where a remembered signature is kept. */
class ViewerViewModelFactory(private val core: PdfCore?, private val filesDir: File) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T =
        ViewerViewModel(core, signatures = FileSignatureStore(File(filesDir, SIGNATURE_FILE_NAME))) as T
}
