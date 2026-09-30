package dev.vitela.pdf.core

data class RenderedPage(val width: Int, val height: Int, val stride: Int, val rgba: ByteArray)

data class SearchHit(val pageIndex: Int, val text: String, val characterBounds: List<TextRect> = emptyList())

/** A page's media box, in PDF points. */
data class PageSize(val widthPt: Double, val heightPt: Double)

/** One file a split creates: zero-based pages [first] to [last], inclusive, written as [fileName]. */
data class SplitPart(val first: Int, val last: Int, val fileName: String)

/**
 * One undoable change to the page layout. Every index is a zero-based position
 * in the document's *current* order, never a page identity.
 */
sealed interface PageEdit {
    /** Takes the page at [from] and puts it at [to]: the position it holds afterwards, as the core reads a move's target. */
    data class Move(val from: Int, val to: Int) : PageEdit
    /** Turns the page at [pageIndex] by [deltaDegrees] (a quarter-turn: 90 or -90). */
    data class Rotate(val pageIndex: Int, val deltaDegrees: Int) : PageEdit
    data class Remove(val pageIndex: Int) : PageEdit
    /** Adds a blank A4 page at [index], before the page there; the page count appends it. */
    data class InsertBlank(val index: Int, val landscape: Boolean = false) : PageEdit
}

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
     * thumb every time a render lands. Read again after every [applyPageEdit]:
     * a move reorders them and a quarter-turn swaps a page's two sides.
     */
    val pageSizes: List<PageSize>

    fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage>
    fun search(query: String): PdfCoreResult<List<SearchHit>>
    fun annotations(): PdfCoreResult<AnnotationSnapshot> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun applyAnnotationEdit(edit: AnnotationEdit): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun undoAnnotations(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    fun redoAnnotations(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Annotations are unavailable in this PDF core."))
    /**
     * Queues one undoable change to the page layout, persisted by the next
     * [saveToBytes]. The core owns the permission question (the assembly bit,
     * and whether the file survives the rewrite a reorder forces); a refusal
     * comes back as the sentence to show.
     */
    fun applyPageEdit(edit: PageEdit): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Organizing pages is unavailable in this PDF core."))
    /**
     * Rebuilds what [renderPage] draws from the pending edits. Rendering reads
     * a preview taken at open, not the live model, so an edit the page itself
     * shows — a moved, turned or removed page — stays invisible until this
     * runs. Annotations are left out: the shell draws those itself.
     */
    fun refreshPreview(): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Previewing edits is unavailable in this PDF core."))
    /** Every AcroForm field the document holds, in the file's own order. Read-only, so available however restricted the document is. */
    fun formFields(): PdfCoreResult<List<FormField>> = PdfCoreResult.Failure(PdfCoreError.Failed("Form fields are unavailable in this PDF core."))
    /**
     * Whether the document lets its fields be filled in: `/P` bit 6, the
     * annotation bit — weaker than the one that lets fields be created or changed.
     */
    fun formFillAllowed(): Boolean = false
    /**
     * Queues one undoable change to what field [fieldId] holds; persisted by
     * the next [saveToBytes]. The page shows it only after [refreshPreview].
     */
    fun setFormFieldValue(fieldId: Long, value: FormFieldValue): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Form fields are unavailable in this PDF core."))
    /**
     * Whether the document lets fields be created or changed: the stronger form
     * permission, asked of the core as one question rather than composed here
     * from the annotation and content answers.
     */
    fun formAuthoringAllowed(): Boolean = false
    /**
     * Queues one undoable new field of [kind] at [rect] on page [pageIndex]; the
     * core picks its id and a unique name. The page shows it only after [refreshPreview].
     */
    fun addFormField(pageIndex: Int, kind: NewFormField, rect: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Creating form fields is unavailable in this PDF core."))
    /** Queues one undoable move of field [fieldId] to [to], on the page it is on. */
    fun moveFormField(fieldId: Long, to: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Moving form fields is unavailable in this PDF core."))
    /** Queues one undoable resize of field [fieldId] to [to], on the page it is on; the same rect as a move, a different intent. */
    fun resizeFormField(fieldId: Long, to: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Resizing form fields is unavailable in this PDF core."))
    /** Whether the document's security context lets a page's own content be rewritten — not the annotation permission. */
    fun contentEditingAllowed(): Boolean = false
    /** The text runs and images page [pageIndex] paints, pending edits included; refused when the document forbids text extraction. */
    fun pageContent(pageIndex: Int): PdfCoreResult<PageContent> = PdfCoreResult.Failure(PdfCoreError.Failed("Editing page content is unavailable in this PDF core."))
    /**
     * Queues one undoable change to what [run] says, keeping its position; a
     * composite-font run is retyped in a standard font instead. A second retype
     * of the same run amends the queued one. The page shows it only after
     * [refreshPreview]; the failure names a character the font cannot show.
     */
    fun retypeTextRun(run: ContentTextRun, text: String): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Editing page content is unavailable in this PDF core."))
    /**
     * Queues one undoable change to the box [image] fills, to [to]; the image
     * is stretched to it. The page shows it only after [refreshPreview].
     */
    fun resizeImage(image: ContentImage, to: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Editing page content is unavailable in this PDF core."))
    /**
     * Queues one undoable move of [image] to [to], the same size somewhere
     * else on its page. The page shows it only after [refreshPreview].
     */
    fun moveImage(image: ContentImage, to: AnnotationRect): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Editing page content is unavailable in this PDF core."))
    /**
     * Queues one undoable removal of [image] from its page. The core keeps
     * what an undo needs to put it back. The page shows it only after [refreshPreview].
     */
    fun removeImage(image: ContentImage): PdfCoreResult<Unit> = PdfCoreResult.Failure(PdfCoreError.Failed("Editing page content is unavailable in this PDF core."))
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

    /**
     * Why this document cannot be compressed at all, as the core's lower-case
     * clause, or null when it can. Cheap: it reads the protection, it saves nothing.
     */
    fun compressionRefusal(): String? = "compression is unavailable in this PDF core"
    /**
     * Whether a compressed save breaks a signature the file carries. Not the
     * ordinary save's question: compressing rewrites a signed file even when
     * nothing was edited.
     */
    fun compressedSaveWillInvalidateSignatures(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Compression is unavailable in this PDF core."))
    /**
     * Saves the session and compresses the result. Not an edit: no undo step,
     * pending edits untouched. [signaturesAcknowledged] is the user's yes to
     * [compressedSaveWillInvalidateSignatures]; without it a signed file is refused.
     */
    fun saveCompressed(preset: CompressPreset, signaturesAcknowledged: Boolean): PdfCoreResult<CompressedCopy> = PdfCoreResult.Failure(PdfCoreError.Failed("Compression is unavailable in this PDF core."))

    /** Whether the document's security context lets its password protection be changed. */
    fun protectionChangeAllowed(): Boolean = false
    /** Whether applying new password protection breaks a signature the file carries. */
    fun protectionWillInvalidateSignatures(): PdfCoreResult<Boolean> = PdfCoreResult.Failure(PdfCoreError.Failed("Password protection is unavailable in this PDF core."))
    /**
     * The session saved with new AES-128 protection under both passwords, which
     * must differ. Not an edit: the open document's own security is untouched.
     * [signaturesAcknowledged] is the user's yes to [protectionWillInvalidateSignatures].
     */
    fun protect(openPassword: String, permissionsPassword: String, signaturesAcknowledged: Boolean): PdfCoreResult<ByteArray> = PdfCoreResult.Failure(PdfCoreError.Failed("Password protection is unavailable in this PDF core."))

    /** Recomputes a full PDF snapshot including every applied annotation edit. */
    fun saveToBytes(): PdfCoreResult<ByteArray> = PdfCoreResult.Failure(PdfCoreError.Failed("Saving is unavailable in this PDF core."))
}

interface PdfCore {
    fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument>
    /**
     * Opens an encrypted PDF after verifying both of its passwords — what a
     * freshly protected file needs to stay fully editable, since one password
     * alone cannot re-apply both roles on its next rewrite.
     */
    fun openWithPasswords(bytes: ByteArray, openPassword: String, permissionsPassword: String): PdfCoreResult<PdfDocument> =
        PdfCoreResult.Failure(PdfCoreError.Failed("Opening with both passwords is unavailable in this PDF core."))
}

/** Implemented by generated packaging sources when native bindings are present. */
interface PdfCoreFactory {
    fun create(): PdfCore
}

object PdfCoreProvider {
    fun create(): PdfCore? = ServiceLoader.load(PdfCoreFactory::class.java).firstOrNull()?.create()
}
