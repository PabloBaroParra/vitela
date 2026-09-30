package dev.vitela.pdf.viewer

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.vitela.pdf.core.AnnotationColor
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.CompressPreset
import dev.vitela.pdf.core.DocumentInfo
import dev.vitela.pdf.core.FormFieldValue
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.SaveSnapshot
import kotlinx.coroutines.CoroutineDispatcher
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
 *
 * [compute] runs the core's work and [io] the shell's file writes; the
 * defaults are the production ones, and tests pass their own scheduler.
 */
class ViewerViewModel(
    private val core: PdfCore?,
    compute: CoroutineDispatcher = Dispatchers.Default,
    io: CoroutineDispatcher = Dispatchers.IO,
) : ViewModel() {
    private val session = ViewerSession({ viewModelScope }, ViewerState(status = availabilityMessage(core), canOpen = core != null), compute, io)
    private val _state = session.state
    val state: StateFlow<ViewerState> = _state.asStateFlow()
    private var sourceBytes: ByteArray? = null
    /** The save target of [sourceBytes], kept beside it so a password retry opens with it. */
    private var sourceTarget: String? = null
    private var pendingReplacement: PendingReplacement? = null
    private var nextDocumentId = 1L

    private val reader = ViewerReader(session)
    private val selection = TextSelecting(session)
    private val pageLayout = PageLayout(session, reader, selection)
    // The lambda reads its features when an undo runs, long after they all exist.
    private val annotations: AnnotationEditing = AnnotationEditing(session, selection, pageLayout) {
        formFilling.reread(it)
        contentEditing.reread(it)
    }
    private val formFilling: FormFilling = FormFilling(session, annotations, pageLayout)
    private val contentEditing: ContentEditing = ContentEditing(session, annotations, pageLayout, selection)
    private val formAuthoring: FormAuthoring = FormAuthoring(session, annotations, pageLayout, formFilling, selection)
    private val organizing = PageOrganizing(session, annotations, pageLayout, selection)
    private val metadata = MetadataEditing(session, annotations)
    private val imageExporting = ImageExporting(session)
    private val pageExtracting = PageExtracting(session)
    private val pageSplitting = PageSplitting(session)
    private val compressing = Compressing(session)
    private val protecting = Protecting(session, ::reopenProtected)
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
            when (val result = withContext(session.compute) { availableCore.openFromBytes(bytes, password) }) {
                is PdfCoreResult.Success -> install(displayName, result.value, saveTarget)
                is PdfCoreResult.Failure -> handleOpenFailure(result.error)
            }
            }
        }
    }

    /** Makes [document] the open one. Called with the document lane held. */
    private suspend fun install(displayName: String, document: PdfDocument, saveTarget: String?) {
        session.document?.close()
        session.document = document
        selection.closeDrag()
        reader.reset()
        val pageCount = document.pageCount
        // A fresh state, not a copy: the previous document's pages,
        // search hits and password flags must not survive. canOpen
        // is true by construction — reaching here proves the core
        // is present.
        _state.value = ViewerState(
            title = displayName,
            pageCount = pageCount,
            pageSizes = withContext(session.compute) { document.pageSizes },
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
        annotations.refresh(document)
        reader.showFirstWindow(pageCount)
    }

    /**
     * Protect's reopen: the file just written, opened under both passwords.
     * No dirty check — the protected bytes carry every pending edit — and the
     * passwords are not retained: [sourceBytes] only ever feeds a retry with
     * the single password the prompt asks for.
     */
    private suspend fun reopenProtected(displayName: String, bytes: ByteArray, openPassword: String, permissionsPassword: String, saveTarget: String?): PdfCoreError? {
        val availableCore = core ?: return PdfCoreError.Failed("Native PDF support is not packaged.")
        return when (val result = withContext(session.compute) { availableCore.openWithPasswords(bytes, openPassword, permissionsPassword) }) {
            is PdfCoreResult.Failure -> result.error
            is PdfCoreResult.Success -> {
                sourceBytes = bytes
                sourceTarget = saveTarget
                install(displayName, result.value, saveTarget)
                null
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
            when (val result = withContext(session.compute) { openDocument.search(query) }) {
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

    // Split into several PDFs
    fun openPageSplit() = pageSplitting.open()
    fun editPageSplit(cuts: String) = pageSplitting.edit(cuts)
    fun dismissPageSplit() = pageSplitting.dismiss()
    /** True once the cuts are accepted and a folder should be asked for; false leaves the dialog open with the reason. */
    suspend fun planPageSplit(): Boolean = pageSplitting.plan()
    fun cancelPageSplit() = pageSplitting.cancel()
    suspend fun splitPages(write: SplitPartWriter) = pageSplitting.split(write)

    // Compress
    fun openCompress() = compressing.open()
    fun selectCompressPreset(preset: CompressPreset) = compressing.select(preset)
    fun dismissCompress() = compressing.dismiss()
    /** The name to suggest in the save picker once smaller bytes exist, or null when there is nothing to write. */
    suspend fun compress(): String? = compressing.compress()
    fun cancelCompress() = compressing.cancel()
    suspend fun writeCompressed(write: CompressedPdfWriter) = compressing.write(write)

    // Protect with a password
    fun openProtect() = protecting.open()
    fun dismissProtect() = protecting.dismiss()
    /** The name to suggest in the save picker once the passwords are accepted, or null when the dialog stays open. */
    fun confirmProtect(openPassword: String, permissionsPassword: String): String? = protecting.confirm(openPassword, permissionsPassword)
    fun cancelProtect() = protecting.cancel()
    suspend fun writeProtected(displayName: String, saveTarget: String?, write: ProtectedPdfWriter) = protecting.write(displayName, saveTarget, write)

    // Organize pages
    fun openOrganize() = organizing.open()
    fun closeOrganize() = organizing.close()
    /** Moves the page at [index] one step toward the front ([delta] -1) or back (+1). */
    fun organizeMove(index: Int, delta: Int) = organizing.move(index, delta)
    /** Turns the page at [index] a quarter: [delta] is 90 or -90. */
    fun organizeRotate(index: Int, delta: Int) = organizing.rotate(index, delta)
    fun organizeDelete(index: Int) = organizing.delete(index)
    fun organizeInsertBlank(index: Int, landscape: Boolean) = organizing.insertBlank(index, landscape)
    /** The grid scrolled the card at [index] into view without a picture. */
    fun organizeThumbnail(index: Int) = organizing.requestThumbnail(index)

    // Form fields
    fun openFormFields() = formFilling.open()
    fun closeFormFields() = formFilling.close()
    /** Fills a field in; [documentId] is the document the row was built for, so a late commit cannot reach another. */
    fun fillFormField(documentId: Long, fieldId: Long, value: FormFieldValue) = formFilling.fill(documentId, fieldId, value)
    /** Arms what the next page tap does to the form — place a field, or move one — or disarms with null. */
    fun armFormField(tap: FormFieldTap?) = formAuthoring.arm(tap)
    fun tapFormField(pageIndex: Int, point: AnnotationPoint) = formAuthoring.tap(pageIndex, point)
    /** Resizes a field in points, keeping its top-left corner; [documentId] is the document the row was built for. */
    fun resizeFormField(documentId: Long, fieldId: Long, width: Double, height: Double) = formAuthoring.resize(documentId, fieldId, width, height)

    // Edit text
    fun openContentEdit() {
        formAuthoring.disarm()
        contentEditing.open()
    }
    fun closeContentEdit() = contentEditing.close()
    /** The reader laid out page [pageIndex] while the mode is armed. */
    fun contentPageShown(pageIndex: Int) = contentEditing.pageShown(pageIndex)
    fun tapContent(pageIndex: Int, point: AnnotationPoint, reach: Double) = contentEditing.tap(pageIndex, point, reach)
    /** Retypes the open editor's run; [documentId] is the document the dialog was built for. */
    fun retypeTextRun(documentId: Long, text: String) = contentEditing.retype(documentId, text)
    fun dismissTextRunEditor() = contentEditing.dismissEditor()

    // Annotations
    /** One mode claims a page tap at a time: choosing a tool leaves Edit text and disarms the form. */
    fun setAnnotationTool(tool: AnnotationTool) {
        contentEditing.close()
        formAuthoring.disarm()
        annotations.setTool(tool)
    }
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
