package dev.vitela.pdf.viewer

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.DocumentInfo
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.SaveSnapshot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

/**
 * Owns the document's lifecycle — open, replace, password retry, search —
 * and is the one entry point the screen talks to. Each feature lives in its
 * own class over the shared [ViewerSession]; the methods below them only
 * forward, so the screen and the tests see a single ViewModel.
 */
class ViewerViewModel(private val core: PdfCore?) : ViewModel() {
    private val session = ViewerSession({ viewModelScope }, ViewerState(status = availabilityMessage(core), canOpen = core != null))
    private val _state = session.state
    val state: StateFlow<ViewerState> = _state.asStateFlow()
    private var sourceBytes: ByteArray? = null
    /** The save target of [sourceBytes], kept beside it so a password retry opens with it. */
    private var sourceTarget: String? = null
    private var pendingReplacement: PendingReplacement? = null
    private var nextDocumentId = 1L

    private val reader = ViewerReader(session)
    private val selection = TextSelecting(session)
    private val annotations = AnnotationEditing(session, selection)
    private val metadata = MetadataEditing(session, annotations)
    private val imageExporting = ImageExporting(session)
    private val pageExtracting = PageExtracting(session)
    private val saving = DocumentSaving(session) { sourceBytes }

    /**
     * [saveTarget] is where **Save** may later write this document back to;
     * omit it for bytes with no writable origin, such as the packaged sample.
     */
    fun open(displayName: String, bytes: ByteArray, password: String? = null, saveTarget: String? = null) {
        if (_state.value.isDirty && session.document != null) {
            pendingReplacement = PendingReplacement(displayName, bytes, password, saveTarget)
            _state.value = _state.value.copy(pendingReplacementTitle = displayName)
            return
        }
        replaceDocument(displayName, bytes, password, saveTarget)
    }

    fun confirmReplacement() {
        val replacement = pendingReplacement ?: return
        pendingReplacement = null
        _state.value = _state.value.copy(pendingReplacementTitle = null)
        replaceDocument(replacement.displayName, replacement.bytes, replacement.password, replacement.saveTarget, discardUnsaved = true)
    }

    fun cancelReplacement() {
        pendingReplacement = null
        if (_state.value.pendingReplacementTitle != null) _state.value = _state.value.copy(pendingReplacementTitle = null)
    }

    private fun replaceDocument(displayName: String, bytes: ByteArray, password: String?, saveTarget: String?, discardUnsaved: Boolean = false) {
        val availableCore = core ?: return
        // Retain the selected bytes and the shell's opaque save target while
        // the session is active. Passwords are never retained after this call.
        viewModelScope.launch {
            session.documentLane.withLock {
            // An edit may have acquired this lane after open() checked state.
            // Check again before replacing the document it just modified.
            if (!discardUnsaved && _state.value.isDirty && session.document != null) {
                pendingReplacement = PendingReplacement(displayName, bytes, password, saveTarget)
                _state.value = _state.value.copy(pendingReplacementTitle = displayName)
                return@withLock
            }
            sourceBytes = bytes
            sourceTarget = saveTarget
            _state.value = _state.value.copy(title = displayName, isLoading = true, needsPassword = false, passwordMessage = null, status = "Opening PDF...")
            when (val result = withContext(Dispatchers.Default) { availableCore.openFromBytes(bytes, password) }) {
                is PdfCoreResult.Success -> {
                    session.document?.close()
                    session.document = result.value
                    selection.closeDrag()
                    reader.reset()
                    val pageCount = result.value.pageCount
                    // A fresh state, not a copy: the previous document's pages,
                    // search hits and password flags must not survive. canOpen
                    // is true by construction — reaching here proves the core
                    // is present.
                    _state.value = ViewerState(
                        title = displayName,
                        pageCount = pageCount,
                        pageSizes = withContext(Dispatchers.Default) { result.value.pageSizes },
                        // Scroll back to the top: the list may still be parked
                        // deep inside the document that was just replaced.
                        scrollTarget = if (pageCount > 0) 0 else null,
                        status = visibleRangeStatus(0, 0, pageCount),
                        canPrint = pageCount > 0,
                        canOpen = true,
                        documentId = nextDocumentId++,
                        // Published only on success: a failed open leaves the
                        // previous document in place, and its target with it.
                        saveTarget = saveTarget,
                    )
                    annotations.refresh(result.value)
                    reader.showFirstWindow(pageCount)
                }
                is PdfCoreResult.Failure -> handleOpenFailure(result.error)
            }
            }
        }
    }

    fun retryPassword(password: String) {
        val bytes = sourceBytes ?: return
        replaceDocument(_state.value.title, bytes, password, sourceTarget)
    }

    /**
     * Dismisses the password prompt without retrying. Without this, an
     * encrypted PDF whose password is unknown had no way out of the prompt
     * (T-085): [sourceBytes] is dropped so a stray retry cannot fire once the
     * user has given up on it.
     */
    fun cancelPassword() {
        if (!_state.value.needsPassword) return
        sourceBytes = null
        sourceTarget = null
        _state.value = _state.value.copy(
            isLoading = false,
            needsPassword = false,
            passwordMessage = null,
            status = "Password entry cancelled.",
        )
    }

    fun reportReadFailure() {
        _state.value = _state.value.copy(status = "Could not read the selected PDF.")
    }

    fun search(query: String) {
        val openDocument = session.document ?: return
        viewModelScope.launch {
            _state.value = _state.value.copy(searchQuery = query, status = "Searching...")
            when (val result = withContext(Dispatchers.Default) { openDocument.search(query) }) {
                is PdfCoreResult.Success -> {
                    val hit = result.value.firstOrNull()
                    _state.value = _state.value.copy(
                        searchHits = result.value,
                        searchIndex = 0,
                        scrollTarget = hit?.pageIndex,
                        status = if (result.value.isEmpty()) "No matches." else "Match 1 of ${result.value.size}.",
                    )
                }
                is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
            }
        }
    }

    fun stepSearch(delta: Int) {
        val hits = _state.value.searchHits
        val index = nextSearchIndex(_state.value.searchIndex, hits.size, delta)
        val hit = hits.getOrNull(index) ?: return
        _state.value = _state.value.copy(searchIndex = index, scrollTarget = hit.pageIndex, status = "Match ${index + 1} of ${hits.size}.")
    }

    // Reader
    fun onReaderPositionChanged(position: ReaderPosition) = reader.onPositionChanged(position)
    fun consumeScrollTarget() = reader.consumeScrollTarget()
    fun navigate(delta: Int) = reader.navigate(delta)
    fun zoomIn() = reader.zoomIn()
    fun zoomOut() = reader.zoomOut()

    // Save and print
    suspend fun printBytes(): ByteArray? = saving.printBytes()
    suspend fun saveSnapshot(): SaveSnapshot? = saving.saveSnapshot()
    fun confirmSaved(snapshot: SaveSnapshot) = saving.confirmSaved(snapshot)
    fun reportSaveFailure() = saving.reportSaveFailure()
    suspend fun inPlaceSave(): InPlaceSave? = saving.inPlaceSave()
    fun reportInPlaceSaveFailure(save: InPlaceSave) = saving.reportInPlaceSaveFailure(save)

    // Document properties
    fun openMetadata() = metadata.open()
    fun editMetadata(draft: DocumentInfo) = metadata.edit(draft)
    fun dismissMetadata() = metadata.dismiss()
    fun applyMetadata() = metadata.apply()

    // Export images
    fun openImageExport() = imageExporting.open()
    fun editImageExport(draft: ImageExportDraft) = imageExporting.edit(draft)
    fun dismissImageExport() = imageExporting.dismiss()
    suspend fun planImageExport(): Boolean = imageExporting.plan()
    fun cancelImageExport() = imageExporting.cancel()
    suspend fun exportImages(write: ImageFileWriter) = imageExporting.export(write)

    // Extract pages
    fun openPageExtract() = pageExtracting.open()
    fun editPageExtract(range: String) = pageExtracting.edit(range)
    fun dismissPageExtract() = pageExtracting.dismiss()
    /** The name to suggest in the save picker once the range is accepted, or null when it was refused. */
    suspend fun planPageExtract(): String? = pageExtracting.plan()
    fun cancelPageExtract() = pageExtracting.cancel()
    suspend fun extractPages(write: ExtractedPdfWriter) = pageExtracting.extract(write)

    // Annotations
    fun setAnnotationTool(tool: AnnotationTool) = annotations.setTool(tool)
    fun selectAnnotation(pageIndex: Int, point: AnnotationPoint) = annotations.select(pageIndex, point)
    fun handlePageGesture(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint>, handleReach: Double) =
        annotations.handlePageGesture(pageIndex, origin, current, points, handleReach)
    fun placeAnnotation(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint> = emptyList()) =
        annotations.place(pageIndex, origin, current, points)
    fun selectImageStamp(bytes: ByteArray) = annotations.selectImageStamp(bytes)
    fun moveSelected(origin: AnnotationPoint, current: AnnotationPoint) = annotations.moveSelected(origin, current)
    fun resizeSelected(corner: HandleCorner, point: AnnotationPoint) = annotations.resizeSelected(corner, point)
    fun growSelected() = annotations.growSelected()
    fun restyleSelected(color: AnnotationColor) = annotations.restyleSelected(color)
    fun deleteSelected() = annotations.deleteSelected()
    fun undoAnnotations() = annotations.undo()
    fun redoAnnotations() = annotations.redo()

    // Text selection
    fun beginTextSelection(pageIndex: Int, point: AnnotationPoint) = selection.begin(pageIndex, point)
    fun extendTextSelection(point: AnnotationPoint) = selection.extend(point)
    fun endTextSelection() = selection.end()
    fun clearTextSelection() = selection.clear()

    private fun handleOpenFailure(error: PdfCoreError) {
        _state.value = _state.value.copy(isLoading = false, needsPassword = error is PdfCoreError.PasswordRequired || error is PdfCoreError.WrongPassword, passwordMessage = if (error is PdfCoreError.WrongPassword) "The password is incorrect. Try again." else null, status = userMessage(error))
    }

    override fun onCleared() {
        selection.closeDrag()
        session.document?.close()
        session.document = null
        super.onCleared()
    }

    private data class PendingReplacement(val displayName: String, val bytes: ByteArray, val password: String?, val saveTarget: String?)
}

private fun availabilityMessage(core: PdfCore?): String = if (core == null) "Native PDF support is not packaged. Build with scripts/package-android.sh and externally supplied PDFium libraries." else "Select a PDF to begin."
