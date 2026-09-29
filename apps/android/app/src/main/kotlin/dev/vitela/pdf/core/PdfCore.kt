package dev.vitela.pdf.core

data class RenderedPage(val width: Int, val height: Int, val stride: Int, val rgba: ByteArray)

data class SearchHit(val pageIndex: Int, val text: String, val characterBounds: List<TextRect> = emptyList())

/** A page's media box, in PDF points. */
data class PageSize(val widthPt: Double, val heightPt: Double)

/** One file a split creates: zero-based pages [first] to [last], inclusive, written as [fileName]. */
data class SplitPart(val first: Int, val last: Int, val fileName: String)

/** A bytes snapshot paired with the document revision it represents. */
data class SaveSnapshot(val bytes: ByteArray, val documentId: Long, val revision: Long)

sealed interface PdfCoreError {
    data object PasswordRequired : PdfCoreError
    data object WrongPassword : PdfCoreError
    data class Failed(val message: String) : PdfCoreError
}

sealed interface PdfCoreResult<out T> {
    data class Success<T>(val value: T) : PdfCoreResult<T>
    data class Failure(val error: PdfCoreError) : PdfCoreResult<Nothing>
}

interface PdfDocument : AutoCloseable {
    val pageCount: Int

    /**
     * Every page's media box, in document order. The continuous reader needs
     * these up front: a page's placeholder has to be laid out at the right
     * height *before* it is rasterized, or the list resizes under the user's
     * thumb every time a render lands.
     */
    val pageSizes: List<PageSize>

    fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage>
    fun search(query: String): PdfCoreResult<List<SearchHit>>
    fun annotations(): PdfCoreResult<AnnotationSnapshot> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun undoAnnotations(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun redoAnnotations(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    /** A page's characters for drag-select; refused when the document forbids text extraction. */
    fun pageCharacters(pageIndex: Int): PdfCoreResult<PageCharacters> = PdfCoreResult.Failure(PdfCoreError.Failed("Text selection is unavailable in this PDF core."))
    /** Core-owned, aspect-ratio-preserving placement policy for an image stamp. */
    fun stampPlacement(imageBytes: ByteArray, anchor: AnnotationPoint): PdfCoreResult<AnnotationRect> = PdfCoreResult.Failure(PdfCoreError.Failed("Image stamps are unavailable in this PDF core."))
    /** Inserts an image-backed stamp using a placement returned by [stampPlacement]. */
    fun insertImageStamp(pageIndex: Int, imageBytes: ByteArray, rect: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Image stamps are unavailable in this PDF core."))

    /** The `/Info` dict as it will be saved: the last queued change, else the file's own. */
    fun documentInfo(): PdfCoreResult<DocumentInfo> = PdfCoreResult.Failure(PdfCoreError.Failed("Document properties are unavailable in this PDF core."))
    /** Whether the document's security context lets its properties be changed. */
    fun metadataEditingAllowed(): Boolean = false
    /** Queues one undoable `/Info` change; persisted by the next [saveToBytes]. */
    fun setDocumentInfo(info: DocumentInfo): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Document properties are unavailable in this PDF core."))

    /** Whether the document's `/P` bits let its pages leave as images (the extraction bit). */
    fun imageExportAllowed(): Boolean = false
    /** Reads a one-based selection such as `"1-3,7"` into ascending zero-based pages; the failure is the core's sentence for the typist. */
    fun parsePageSelection(input: String): PdfCoreResult<List<Int>> = PdfCoreResult.Failure(PdfCoreError.Failed("Image export is unavailable in this PDF core."))
    /** The file name page [pageIndex] is exported under, named after [displayName]; always one path component. */
    fun pageImageFileName(displayName: String, pageIndex: Int, format: ImageExportFormat): PdfCoreResult<String> = PdfCoreResult.Failure(PdfCoreError.Failed("Image export is unavailable in this PDF core."))
    /** The first of [pages] whose whole-page raster at [dpi] is over the render ceiling, or null when all fit. */
    fun firstPageTooLargeToExport(pages: List<Int>, dpi: Int): PdfCoreResult<Int?> = PdfCoreResult.Failure(PdfCoreError.Failed("Image export is unavailable in this PDF core."))
    /** One page rendered at [dpi] and encoded: the bytes of the file to write. */
    fun exportPageImage(pageIndex: Int, dpi: Int, format: ImageExportFormat): PdfCoreResult<ByteArray> = PdfCoreResult.Failure(PdfCoreError.Failed("Image export is unavailable in this PDF core."))

    /** Whether the document's `/P` bits let its pages leave for a new PDF — the extraction bit, not the assembly one. */
    fun pageExtractionAllowed(): Boolean = false
    /** Whether the document survives a full rewrite: false for an encrypted file opened with only one of its two passwords. */
    fun fullRewriteAllowed(): Boolean = false
    /** Whether the source carries a signature, which the extracted copy cannot keep valid. */
    fun extractSourceIsSigned(): Boolean = false
    /** The bytes of a new PDF holding only [pages] (ascending, zero-based); the open document is untouched. */
    fun extractPages(pages: List<Int>): PdfCoreResult<ByteArray> = PdfCoreResult.Failure(PdfCoreError.Failed("Extracting pages is unavailable in this PDF core."))

    /**
     * The parts that cutting after each typed page (one-based, `"3,7"`) makes,
     * named after [displayName]. The failure is the core's sentence, including
     * a blank field and a one-page document.
     */
    fun planSplit(cuts: String, displayName: String): PdfCoreResult<List<SplitPart>> = PdfCoreResult.Failure(PdfCoreError.Failed("Splitting is unavailable in this PDF core."))

    /** Recomputes a full PDF snapshot including every applied annotation edit. */
    fun saveToBytes(): PdfCoreResult<ByteArray> = PdfCoreResult.Failure(PdfCoreError.Failed("Saving is unavailable in this PDF core."))
}

interface PdfCore {
    fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument>
}

/** Implemented by generated packaging sources when native bindings are present. */
interface PdfCoreFactory {
    fun create(): PdfCore
}

object PdfCoreProvider {
    fun create(): PdfCore? = ServiceLoader.load(PdfCoreFactory::class.java).firstOrNull()?.create()
}
