package dev.vitela.pdf.core

/** How a text run's font is stored, which decides whether a retype can keep it. */
enum class ContentFontKind { Standard14, EmbeddedSimple, EmbeddedComposite }

/**
 * One run of text a page's own content stream paints, as the core parsed it
 * with every pending edit layered on top: a retyped run keeps its [id] and
 * reports the text it now shows. [pageIndex] is the page's zero-based position
 * in the current order; [bounds] are in PDF points.
 *
 * Never edited on its own: it is read as part of a [ContentTextBlock], and the
 * block is handed back unchanged to the block edits, which match it against
 * exactly this parse.
 */
data class ContentTextRun(
    val id: Long,
    val pageIndex: Int,
    val bounds: AnnotationRect,
    val resourceFontName: String,
    val fontKind: ContentFontKind,
    val text: String,
)

/**
 * Text runs that read as one line — on one line, in one font, no more than a
 * word gap apart — as the core grouped them (B25 T-207). A report generator
 * paints one phrase with several show-text operators; the reader sees, selects,
 * moves, retypes and deletes it as one piece, each an undo step of its own.
 *
 * [runs] are left to right, exactly as the page's content read them, and every
 * run of a page is in exactly one block. [bounds] is the union of their boxes and
 * [text] their text joined, with a space put back where the file left a word gap
 * out of it. A one-run block is just a block.
 *
 * Handed back unchanged to [PdfDocument.retypeTextBlock],
 * [PdfDocument.moveTextBlock] and [PdfDocument.removeTextBlock]: the core
 * matches the edit against exactly this parse.
 */
data class ContentTextBlock(
    val runs: List<ContentTextRun>,
    val bounds: AnnotationRect,
    val text: String,
) {
    /** The first run's id, which names the block: a retyped or moved block keeps it. */
    val id: Long get() = runs.first().id

    /** The page the block is on, its runs' page. */
    val pageIndex: Int get() = runs.first().pageIndex
}

/**
 * One image a page's own content stream paints, as the core parsed it with
 * every pending edit layered on top: a resized or moved image keeps its [id]
 * and reports the box it now fills. [resourceName] is its `/XObject` name, or null
 * for an inline image; [bounds] are in PDF points.
 *
 * Handed back unchanged to [PdfDocument.resizeImage],
 * [PdfDocument.moveImage] and [PdfDocument.removeImage], like a [ContentTextRun].
 */
data class ContentImage(
    val id: Long,
    val pageIndex: Int,
    val bounds: AnnotationRect,
    val resourceName: String?,
)

/** How a vector path is painted, which decides where a tap on it means it. */
enum class PathPaint { Stroke, Fill, FillAndStroke }

/**
 * One vector path a page's own content stream paints — a table border, a rule,
 * a shaded box — as the core parsed it with every pending edit layered on top:
 * a moved or reshaped path keeps its [id] and reports the box it now fills.
 * [bounds] are in PDF points and cover the geometry only, not the stroke, so a
 * hairline rule is a box of height zero.
 *
 * Handed back unchanged to [PdfDocument.movePath], [PdfDocument.resizePath]
 * and [PdfDocument.removePath], like a [ContentImage].
 */
data class ContentPath(
    val id: Long,
    val pageIndex: Int,
    val bounds: AnnotationRect,
    val paint: PathPaint,
)

/**
 * What page content editing can act on: one page's text runs and the [blocks]
 * they group into, images and vector paths, from one parse. [paths] and
 * [blocks] default to none, so a core that cannot read them still describes a page.
 */
data class PageContent(
    val textRuns: List<ContentTextRun>,
    val images: List<ContentImage>,
    val paths: List<ContentPath> = emptyList(),
    val blocks: List<ContentTextBlock> = emptyList(),
)

/**
 * What a marquee or a tap-to-add gathered on one page (B25 T-208): text
 * [blocks], [images] and vector [paths], each exactly as it was last read, so
 * the core can match the edit against that same parse. A block is sent as its
 * runs. A selection edits one page at a time: [pageIndex] is that page, and the
 * core refuses members from two.
 *
 * Handed to [PdfDocument.moveContentSelection] and
 * [PdfDocument.removeContentSelection], each of which is one undo step.
 */
data class ContentSelection(
    val pageIndex: Int,
    val blocks: List<ContentTextBlock> = emptyList(),
    val images: List<ContentImage> = emptyList(),
    val paths: List<ContentPath> = emptyList(),
) {
    /** How many items it holds, a block counting as one. */
    val count: Int get() = blocks.size + images.size + paths.size
}

/**
 * Page content copied out of a page (B25 T-209): a self-contained one-page PDF,
 * [pdf], and the box the copied items covered on their page, [bounds] in PDF
 * points. Holds no id of the document it came from, so the shell keeps it past
 * a page edit and past the document, and it pastes into another one.
 *
 * Not a data class: two copies of the same items are two clips, and array
 * equality would compare contents.
 */
class ContentClip(val pdf: ByteArray, val bounds: AnnotationRect)
