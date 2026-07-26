package dev.vitela.pdf

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.viewer.ViewerViewModel

class ViewerViewModelFactory(private val core: PdfCore?) : ViewModelProvider.Factory {
    @Suppress("UNCHECKED_CAST")
    override fun <T : ViewModel> create(modelClass: Class<T>): T = ViewerViewModel(core) as T
}
