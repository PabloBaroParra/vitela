package dev.vitela.pdf.core

data class AnnotationColor(val red: Int, val green: Int, val blue: Int)

data class AnnotationPoint(val x: Double, val y: Double)

data class AnnotationRect(val x: Double, val y: Double, val width: Double, val height: Double)

data class TextRect(val x: Double, val y: Double, val width: Double, val height: Double)

data class TextRun(val text: String, val characterBounds: List<TextRect>)

enum class AnnotationKind { Highlight, Underline, Strikeout, Ink, Shape, TextNote, Stamp }

data class Annotation(
    val id: Long,
    val pageIndex: Int,
    val kind: AnnotationKind,
    val rect: AnnotationRect?,
    val color: AnnotationColor?,
    val points: List<AnnotationPoint> = emptyList(),
)

sealed interface AnnotationEdit {
    data class Add(val annotation: Annotation) : AnnotationEdit
    data class Move(val id: Long, val dx: Double, val dy: Double) : AnnotationEdit
    data class Resize(val id: Long, val rect: AnnotationRect) : AnnotationEdit
    data class Restyle(val id: Long, val color: AnnotationColor) : AnnotationEdit
    data class Remove(val id: Long) : AnnotationEdit
}

data class AnnotationSnapshot(
    val annotations: List<Annotation>,
    val editingAllowed: Boolean,
    val canUndo: Boolean,
    val canRedo: Boolean,
)
