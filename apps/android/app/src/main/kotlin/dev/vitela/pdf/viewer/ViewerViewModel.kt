package dev.vitela.pdf.viewer

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.AnnotationEdit
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.TextRect
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext

class ViewerViewModel(private val core: PdfCore?) : ViewModel() {
    private val _state = MutableStateFlow(ViewerState(status = availabilityMessage(core), canOpen = core != null))
    val state: StateFlow<ViewerState> = _state.asStateFlow()
    private var sourceBytes: ByteArray? = null
    private var document: PdfDocument? = null
    private var pendingReplacement: PendingReplacement? = null
    private var stampBytes: ByteArray? = null
    private var nextDocumentId = 1L
    /** Serializes mutation, save, and replacement snapshots for this ViewModel's lifecycle. */
    private val documentLane = Mutex()

    /**
     * Pages with a render in flight, mapped to the [layoutGeneration] they
     * were started for, so a scroll tick never queues the same page twice and
     * a render left over from the previous layout cannot evict the entry
     * belonging to its replacement.
     */
    private val inFlight = mutableMapOf<Int, Int>()

    /**
     * The window a finished render is still wanted for. A render that lands
     * after the user scrolled past its page must be dropped, not cached —
     * otherwise fast scrolling grows the resident set past [CACHE_PAGES].
     */
    private var cacheWindow: IntRange = IntRange.EMPTY

    /** Slot width every cached bitmap was rasterized for. Zero until the reader is laid out. */
    private var renderWidthPx = 0
    private var renderZoomFactor = DEFAULT_ZOOM_FACTOR

    /**
     * Bumped whenever [renderWidthPx] changes. Fit-to-width ties a bitmap to
     * the width it was rendered for, so a rotation invalidates the whole cache
     * *and* every render already in flight; the generation is how a finished
     * render knows it is answering a question nobody is asking any more.
     */
    private var layoutGeneration = 0

    fun open(displayName: String, bytes: ByteArray, password: String? = null) {
        if (_state.value.isDirty && document != null) {
            pendingReplacement = PendingReplacement(displayName, bytes, password)
            _state.value = _state.value.copy(pendingReplacementTitle = displayName)
            return
        }
        replaceDocument(displayName, bytes, password)
    }

    fun confirmReplacement() {
        val replacement = pendingReplacement ?: return
        pendingReplacement = null
        _state.value = _state.value.copy(pendingReplacementTitle = null)
        replaceDocument(replacement.displayName, replacement.bytes, replacement.password, discardUnsaved = true)
    }

    fun cancelReplacement() {
        pendingReplacement = null
        if (_state.value.pendingReplacementTitle != null) _state.value = _state.value.copy(pendingReplacementTitle = null)
    }

    private fun replaceDocument(displayName: String, bytes: ByteArray, password: String?, discardUnsaved: Boolean = false) {
        val availableCore = core ?: return
        // Retain only the selected bytes while the session is active. SAF URIs
        // are not paths and passwords are never retained after this call.
        viewModelScope.launch {
            documentLane.withLock {
            // An edit may have acquired this lane after open() checked state.
            // Check again before replacing the document it just modified.
            if (!discardUnsaved && _state.value.isDirty && document != null) {
                pendingReplacement = PendingReplacement(displayName, bytes, password)
                _state.value = _state.value.copy(pendingReplacementTitle = displayName)
                return@withLock
            }
            sourceBytes = bytes
            _state.value = _state.value.copy(title = displayName, isLoading = true, needsPassword = false, passwordMessage = null, status = "Opening PDF...")
            when (val result = withContext(Dispatchers.Default) { availableCore.openFromBytes(bytes, password) }) {
                is PdfCoreResult.Success -> {
                    document?.close()
                    document = result.value
                    inFlight.clear()
                    cacheWindow = IntRange.EMPTY
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
                    )
                    refreshAnnotations(result.value)
                    // Drive the first window from here rather than waiting for
                    // the reader: opening a second document while the list is
                    // already parked at page 0 reports an unchanged position,
                    // which the reader's distinctUntilChanged would swallow.
                    if (pageCount > 0) {
                        onReaderPositionChanged(ReaderPosition(0, 0, 0, renderWidthPx, _state.value.zoomFactor))
                    }
                }
                is PdfCoreResult.Failure -> handleOpenFailure(result.error)
            }
            }
        }
    }

    fun retryPassword(password: String) {
        val bytes = sourceBytes ?: return
        replaceDocument(_state.value.title, bytes, password)
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

    /**
     * Called by the reader whenever what is on screen changes. This is the
     * single driver of rendering: it decides what to rasterize, what to keep,
     * and what to evict, exactly like the GTK shell's viewport tick.
     */
    fun onReaderPositionChanged(position: ReaderPosition) {
        if (document == null) return
        val pageCount = _state.value.pageCount
        if (pageCount == 0) return
        // Effects from the old composition may report once while a zoom
        // recomposes. They must not restore its retired render parameters.
        if (position.zoomFactor != _state.value.zoomFactor) return
        if (position.viewportWidthPx > 0 && (position.viewportWidthPx != renderWidthPx || position.zoomFactor != renderZoomFactor)) {
            // A rotation, resize, or zoom creates a new bitmap generation. The
            // old cache remains only as a temporary, cache-window-bound bridge.
            renderWidthPx = position.viewportWidthPx
            renderZoomFactor = position.zoomFactor
            layoutGeneration += 1
            inFlight.clear()
            _state.value = _state.value.withInvalidatedPageBitmaps(cacheWindow)
        }
        cacheWindow = pageWindow(position.first, position.last, pageCount, CACHE_PAGES)
        _state.value = _state.value.copy(
            pageIndex = boundedPageIndex(position.current, pageCount),
            status = visibleRangeStatus(position.first, position.last, pageCount),
        ).withRetainedPageBitmaps(cacheWindow)
        // Nothing has been measured yet, so there is no width to fit to.
        // The reader's first layout pass calls back with a real one.
        if (renderWidthPx <= 0) return
        for (pageIndex in renderOrder(position.first, position.last, pageCount)) {
            if (pageIndex !in _state.value.pages) renderPage(pageIndex)
        }
    }

    /** Consumed by the reader once it has scrolled, so the target fires once. */
    fun consumeScrollTarget() {
        if (_state.value.scrollTarget != null) _state.value = _state.value.copy(scrollTarget = null)
    }

    fun navigate(delta: Int) {
        if (_state.value.pageCount == 0) return
        _state.value = _state.value.copy(scrollTarget = boundedPageIndex(_state.value.pageIndex + delta, _state.value.pageCount))
    }

    fun zoomIn() = changeZoom(zoomIn(_state.value.zoomFactor))

    fun zoomOut() = changeZoom(zoomOut(_state.value.zoomFactor))

    fun search(query: String) {
        val openDocument = document ?: return
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

    /** Bytes for printing/sharing, recomputed to include every applied annotation edit. */
    suspend fun printBytes(): ByteArray? {
        return documentLane.withLock {
            val openDocument = document ?: return@withLock sourceBytes
            when (val result = withContext(Dispatchers.Default) { openDocument.saveToBytes() }) {
                is PdfCoreResult.Success -> result.value
                is PdfCoreResult.Failure -> sourceBytes
            }
        }
    }

    suspend fun saveSnapshot(): dev.vitela.pdf.core.SaveSnapshot? = documentLane.withLock {
        val openDocument = document ?: return@withLock null
        when (val result = withContext(Dispatchers.Default) { openDocument.saveToBytes() }) {
            is PdfCoreResult.Success -> dev.vitela.pdf.core.SaveSnapshot(result.value, _state.value.documentId, _state.value.revision)
            is PdfCoreResult.Failure -> {
                _state.value = _state.value.copy(status = userMessage(result.error))
                null
            }
        }
    }

    fun confirmSaved(snapshot: dev.vitela.pdf.core.SaveSnapshot) {
        viewModelScope.launch {
            documentLane.withLock {
                if (_state.value.matches(snapshot)) {
                    _state.value = _state.value.copy(isDirty = false, status = "Saved.")
                }
            }
        }
    }

    fun reportSaveFailure() {
        _state.value = _state.value.copy(status = "Could not save the PDF.")
    }

    fun setAnnotationTool(tool: AnnotationTool) {
        if (!_state.value.annotationEditingAllowed) return
        val selection = _state.value.textSelection
        if (selection != null && tool in setOf(AnnotationTool.Highlight, AnnotationTool.Underline, AnnotationTool.Strikeout)) {
            applyAnnotationEdits(markupTextSelection(tool, selection).map(AnnotationEdit::Add))
            _state.value = _state.value.copy(activeAnnotationTool = AnnotationTool.Pointer, textSelection = null)
        } else {
            _state.value = _state.value.copy(activeAnnotationTool = tool)
        }
    }

    fun selectAnnotation(pageIndex: Int, point: AnnotationPoint) {
        val selected = annotationAt(_state.value.annotations, pageIndex, point)
        _state.value = _state.value.copy(selectedAnnotationId = selected?.id)
    }

    fun handlePageGesture(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint>, handleReach: Double) {
        if (_state.value.activeAnnotationTool != AnnotationTool.Pointer) {
            placeAnnotation(pageIndex, origin, current, points)
            return
        }
        val selected = selectedAnnotation()?.takeIf { it.pageIndex == pageIndex }
        when (val mode = selected?.let { dragModeAt(it, origin, handleReach) }) {
            DragMode.Move -> moveSelected(origin, current)
            is DragMode.Resize -> resizeSelected(mode.corner, current)
            null -> {
                val hit = annotationAt(_state.value.annotations, pageIndex, origin)
                if (hit != null) _state.value = _state.value.copy(selectedAnnotationId = hit.id, textSelection = null) else selectText(pageIndex, origin, current)
            }
        }
    }

    fun placeAnnotation(pageIndex: Int, origin: AnnotationPoint, current: AnnotationPoint, points: List<AnnotationPoint> = emptyList()) {
        val tool = _state.value.activeAnnotationTool
        if (!_state.value.annotationEditingAllowed || tool == AnnotationTool.Pointer || (tool == AnnotationTool.Ink && points.size < 2)) return
        if (tool == AnnotationTool.Stamp) {
            val image = stampBytes ?: run {
                _state.value = _state.value.copy(status = "Choose an image before placing a stamp.")
                return
            }
            insertImageStamp(pageIndex, image, origin)
        } else {
            applyAnnotationEdit(AnnotationEdit.Add(placementAnnotation(tool, pageIndex, origin, current, points)))
        }
        _state.value = _state.value.copy(activeAnnotationTool = AnnotationTool.Pointer)
    }

    fun selectImageStamp(bytes: ByteArray) {
        if (!_state.value.annotationEditingAllowed) return
        stampBytes = bytes
        _state.value = _state.value.copy(activeAnnotationTool = AnnotationTool.Stamp, status = "Tap a page to place the image stamp.")
    }

    fun moveSelected(origin: AnnotationPoint, current: AnnotationPoint) {
        val selected = selectedAnnotation() ?: return
        if (origin == current) return
        applyAnnotationEdit(AnnotationEdit.Move(selected.id, current.x - origin.x, current.y - origin.y))
    }

    fun resizeSelected(corner: HandleCorner, point: AnnotationPoint) {
        val selected = selectedAnnotation() ?: return
        val rect = selected.rect ?: return
        applyAnnotationEdit(AnnotationEdit.Resize(selected.id, resizedRect(rect, corner, point)))
    }

    fun growSelected() {
        val selected = selectedAnnotation() ?: return
        selected.rect?.let { applyAnnotationEdit(AnnotationEdit.Resize(selected.id, grownRect(it))) }
    }

    fun restyleSelected(color: dev.vitela.pdf.core.AnnotationColor) {
        selectedAnnotation()?.takeIf { it.supportsRestyle }?.let { applyAnnotationEdit(AnnotationEdit.Restyle(it.id, color)) }
    }

    fun deleteSelected() {
        selectedAnnotation()?.let { annotation ->
            applyAnnotationEdit(AnnotationEdit.Remove(annotation.id))
            _state.value = _state.value.copy(selectedAnnotationId = null)
        }
    }

    fun undoAnnotations() = applyHistory(undo = true)
    fun redoAnnotations() = applyHistory(undo = false)

    fun selectText(pageIndex: Int, start: AnnotationPoint, end: AnnotationPoint) {
        val openDocument = document ?: return
        viewModelScope.launch {
            when (val result = withContext(Dispatchers.Default) { openDocument.textRuns(pageIndex) }) {
                is PdfCoreResult.Success -> {
                    val selection = AnnotationRect(minOf(start.x, end.x), minOf(start.y, end.y), kotlin.math.abs(end.x - start.x), kotlin.math.abs(end.y - start.y))
                    val lines = result.value.flatMap { it.characterBounds }.filter { it.overlaps(selection) }.mergeLines()
                    _state.value = _state.value.copy(textSelection = TextSelection(pageIndex, lines).takeIf { it.rects.isNotEmpty() })
                }
                is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
            }
        }
    }

    private fun renderPage(pageIndex: Int) {
        val openDocument = document ?: return
        val generation = layoutGeneration
        if (inFlight[pageIndex] == generation) return
        inFlight[pageIndex] = generation
        val dpi = renderDpi(_state.value.pageSizes.getOrNull(pageIndex), renderWidthPx, renderZoomFactor)
        viewModelScope.launch {
            val result = withContext(Dispatchers.Default) { openDocument.renderPage(pageIndex, dpi) }
            if (inFlight[pageIndex] == generation) inFlight.remove(pageIndex)
            // A render outlives the document that started it when the user
            // opens another file mid-scroll, and outlives its own layout when
            // the device rotates. Either way the bitmap belongs to nobody.
            if (document !== openDocument || !acceptsRenderCompletion(generation, layoutGeneration, pageIndex, cacheWindow)) return@launch
            when (result) {
                is PdfCoreResult.Success -> {
                    val bitmap = result.value.toImageBitmap() ?: return@launch
                    _state.value = _state.value.withReplacementPage(pageIndex, bitmap)
                }
                is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
            }
        }
    }

    private fun handleOpenFailure(error: PdfCoreError) {
        _state.value = _state.value.copy(isLoading = false, needsPassword = error is PdfCoreError.PasswordRequired || error is PdfCoreError.WrongPassword, passwordMessage = if (error is PdfCoreError.WrongPassword) "The password is incorrect. Try again." else null, status = userMessage(error))
    }

    private fun changeZoom(zoomFactor: Double) {
        if (document == null || zoomFactor == _state.value.zoomFactor) return
        // Page geometry changes immediately. Keep cache-window pages beneath the
        // next generation until their sharp replacements arrive.
        layoutGeneration += 1
        inFlight.clear()
        renderWidthPx = 0
        _state.value = _state.value.copy(zoomFactor = zoomFactor).withInvalidatedPageBitmaps(cacheWindow)
    }

    private fun selectedAnnotation() = _state.value.selectedAnnotationId?.let { id -> _state.value.annotations.lastOrNull { it.id == id } }

    private fun applyAnnotationEdit(edit: AnnotationEdit) = applyAnnotationEdits(listOf(edit))

    private fun applyAnnotationEdits(edits: List<AnnotationEdit>) {
        val openDocument = document ?: return
        if (!_state.value.annotationEditingAllowed || edits.isEmpty()) return
        viewModelScope.launch {
            documentLane.withLock {
                if (document !== openDocument) return@withLock
                var applied = false
                for (edit in edits) {
                    when (val result = withContext(Dispatchers.Default) { openDocument.applyAnnotationEdit(edit) }) {
                        is PdfCoreResult.Success -> applied = true
                        is PdfCoreResult.Failure -> {
                            if (applied) {
                                _state.value = _state.value.copy(isDirty = true, revision = _state.value.revision + 1)
                                refreshAnnotations(openDocument)
                            }
                            _state.value = _state.value.copy(status = userMessage(result.error))
                            return@withLock
                        }
                    }
                }
                _state.value = _state.value.copy(isDirty = true, revision = _state.value.revision + 1)
                refreshAnnotations(openDocument)
            }
        }
    }

    private fun applyHistory(undo: Boolean) {
        val openDocument = document ?: return
        viewModelScope.launch {
            documentLane.withLock {
                if (document !== openDocument) return@withLock
                val result = withContext(Dispatchers.Default) { if (undo) openDocument.undoAnnotations() else openDocument.redoAnnotations() }
                when (result) {
                    is PdfCoreResult.Success -> if (result.value) {
                        _state.value = _state.value.copy(isDirty = true, revision = _state.value.revision + 1)
                        refreshAnnotations(openDocument)
                    }
                    is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
                }
            }
        }
    }

    private suspend fun refreshAnnotations(openDocument: PdfDocument) {
        when (val result = withContext(Dispatchers.Default) { openDocument.annotations() }) {
            is PdfCoreResult.Success -> _state.value = _state.value.copy(
                annotations = result.value.annotations,
                annotationEditingAllowed = result.value.editingAllowed,
                canUndoAnnotations = result.value.canUndo,
                canRedoAnnotations = result.value.canRedo,
                selectedAnnotationId = _state.value.selectedAnnotationId?.takeIf { id -> result.value.annotations.any { it.id == id } },
            )
            is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
        }
    }

    private fun insertImageStamp(pageIndex: Int, imageBytes: ByteArray, anchor: AnnotationPoint) {
        val openDocument = document ?: return
        viewModelScope.launch {
            documentLane.withLock {
                if (document !== openDocument) return@withLock
                when (val placement = withContext(Dispatchers.Default) { openDocument.stampPlacement(imageBytes, anchor) }) {
                    is PdfCoreResult.Success -> when (val result = withContext(Dispatchers.Default) { openDocument.insertImageStamp(pageIndex, imageBytes, placement.value) }) {
                        is PdfCoreResult.Success -> {
                            _state.value = _state.value.copy(isDirty = true, revision = _state.value.revision + 1)
                            refreshAnnotations(openDocument)
                        }
                        is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(result.error))
                    }
                    is PdfCoreResult.Failure -> _state.value = _state.value.copy(status = userMessage(placement.error))
                }
            }
        }
    }

    override fun onCleared() {
        document?.close()
        document = null
        super.onCleared()
    }

    private data class PendingReplacement(val displayName: String, val bytes: ByteArray, val password: String?)
}

private fun TextRect.overlaps(selection: AnnotationRect): Boolean =
    x + width >= selection.x && x <= selection.x + selection.width && y + height >= selection.y && y <= selection.y + selection.height

private fun List<TextRect>.mergeLines(): List<TextRect> = groupBy { kotlin.math.round(it.y * 10) / 10 }.values.map { line ->
    val left = line.minOf { it.x }; val right = line.maxOf { it.x + it.width }
    TextRect(left, line.minOf { it.y }, right - left, line.maxOf { it.y + it.height } - line.minOf { it.y })
}

private fun ViewerState.withInvalidatedPageBitmaps(window: IntRange): ViewerState {
    val bitmaps = invalidatePageBitmaps(PageBitmaps(pages, bridgePages), window)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun ViewerState.withRetainedPageBitmaps(window: IntRange): ViewerState {
    val bitmaps = retainPageBitmaps(PageBitmaps(pages, bridgePages), window)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun ViewerState.withReplacementPage(pageIndex: Int, page: androidx.compose.ui.graphics.ImageBitmap): ViewerState {
    val bitmaps = replacePageBitmap(PageBitmaps(pages, bridgePages), pageIndex, page)
    return copy(pages = bitmaps.pages, bridgePages = bitmaps.bridges)
}

private fun availabilityMessage(core: PdfCore?): String = if (core == null) "Native PDF support is not packaged. Build with scripts/package-android.sh and externally supplied PDFium libraries." else "Select a PDF to begin."

private fun userMessage(error: PdfCoreError): String = when (error) {
    PdfCoreError.PasswordRequired, PdfCoreError.WrongPassword -> "This document requires a password."
    is PdfCoreError.Failed -> error.message
}
