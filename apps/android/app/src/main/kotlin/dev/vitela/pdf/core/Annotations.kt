package dev.vitela.pdf.core

data class AnnotationColor(val red: Int, val green: Int, val blue: Int)

data class AnnotationPoint(val x: Double, val y: Double)

data class AnnotationRect(val x: Double, val y: Double, val width: Double, val height: Double)

data class TextRect(val x: Double, val y: Double, val width: Double, val height: Double)

enum class AnnotationKind { Highlight, Underline, Strikeout, Ink, Shape, TextNote, Stamp, FreeText }

/**
 * One laid-out line of a text box in the box's own frame: origin at its
 * top-left corner, y growing downwards, in points. The core wraps; shells
 * only draw.
 */
data class FreeTextLine(val text: String, val xPt: Double, val baselineFromTopPt: Double)

/** The core's layout of a text box's contents: what the saved file will paint. */
data class FreeTextLayout(val fontSizePt: Double, val lines: List<FreeTextLine>)

data class Annotation(
    val id: Long,
    val pageIndex: Int,
    val kind: AnnotationKind,
    val rect: AnnotationRect?,
    val color: AnnotationColor?,
    val points: List<AnnotationPoint> = emptyList(),
    /** A Note's or text box's text, as typed; null for every other kind. */
    val contents: String? = null,
    /** A text box's lines as the core wrapped them; null for every other kind. */
    val layout: FreeTextLayout? = null,
)

sealed interface AnnotationEdit {
    data class Add(val annotation: Annotation) : AnnotationEdit
    data class Move(val id: Long, val dx: Double, val dy: Double) : AnnotationEdit
    data class Resize(val id: Long, val rect: AnnotationRect) : AnnotationEdit
    data class Restyle(val id: Long, val color: AnnotationColor) : AnnotationEdit
    data class Remove(val id: Long) : AnnotationEdit
    /** Retypes a text box; the core refuses blank text and characters Helvetica cannot show. */
    data class SetContents(val id: Long, val contents: String) : AnnotationEdit
}

data class AnnotationSnapshot(
    val annotations: List<Annotation>,
    val editingAllowed: Boolean,
    val canUndo: Boolean,
    val canRedo: Boolean,
)
