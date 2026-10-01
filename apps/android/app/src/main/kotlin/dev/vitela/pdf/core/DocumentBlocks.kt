package dev.vitela.pdf.core

/** Which PDF a block's pages come from — pdf-ffi's `FfiBlockSource`. */
sealed interface BlockSource {
    /** The file that was opened. */
    data object Base : BlockSource

    /** Pages that were never in a PDF: inserted blank pages. */
    data object Blank : BlockSource

    /** A PDF added from Organize; [id] is its [ImportReport.sourceId]. */
    data class Imported(val id: Long) : BlockSource
}

/**
 * One contiguous run of pages from the same source — pdf-ffi's
 * `FfiDocumentBlock`. [part] is 1-based and set only when [source] is split
 * across more than one block; [start] is a position in the current order.
 */
data class DocumentBlock(val source: BlockSource, val part: Int?, val start: Int, val count: Int)
