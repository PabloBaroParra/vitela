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
 * What page content editing can act on: one page's text runs, images and vector
 * paths, from one parse. [paths] defaults to none, so a core that cannot read
 * them still describes a page.
 */
data class PageContent(
    val textRuns: List<ContentTextRun>,
    val images: List<ContentImage>,
    val paths: List<ContentPath> = emptyList(),
)
