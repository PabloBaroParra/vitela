package dev.vitela.pdf.core

/** How a text run's font is stored, which decides whether a retype can keep it. */
enum class ContentFontKind { Standard14, EmbeddedSimple, EmbeddedComposite }

/**
 * One run of text a page's own content stream paints, as the core parsed it
 * with every pending edit layered on top: a retyped run keeps its [id] and
 * reports the text it now shows. [pageIndex] is the page's zero-based position
 * in the current order; [bounds] are in PDF points.
 *
 * Handed back unchanged to [PdfDocument.retypeTextRun] and
 * [PdfDocument.removeTextRun]: the core matches the edit against exactly this parse.
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

/** What page content editing can act on: one page's text runs and images, from one parse. */
data class PageContent(
    val textRuns: List<ContentTextRun>,
    val images: List<ContentImage>,
)
