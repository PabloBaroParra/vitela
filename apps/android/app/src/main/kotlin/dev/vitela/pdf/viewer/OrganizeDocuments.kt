package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.BlockSource
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit

/*
 * The Documents view of Organize: one card per run of pages from the same PDF,
 * as the core groups them. A block is moved, turned or deleted whole, each one
 * undoable step — the same block edits the Linux shell's Documents view makes,
 * by arrow buttons here for the reason the Pages view gives (OrganizeGrid).
 *
 * Nothing about a block is kept across an edit: a move can merge two runs into
 * one or split one in two, so the core is asked again after every change.
 */

/** "report.pdf" or "report.pdf — Part 2": what the card is called. Names are the shell's, the core only knows ids. */
internal fun blockTitle(block: DocumentBlock, baseName: String, importedNames: Map<Long, String>): String {
    val name = when (val source = block.source) {
        BlockSource.Base -> baseName
        BlockSource.Blank -> "Blank pages"
        is BlockSource.Imported -> importedNames[source.id] ?: "Imported PDF"
    }
    return block.part?.let { "$name — Part $it" } ?: name
}

/** "7 pages · 3–9": how big the block is and where it sits now. */
internal fun blockMeta(block: DocumentBlock): String {
    val first = block.start + 1
    return if (block.count == 1) "1 page · $first" else "${block.count} pages · $first–${block.start + block.count}"
}

/**
 * The move that takes [block] past its neighbour: before the previous block
 * ([delta] -1) or after the next one (+1). Null at either end, and for a block
 * [blocks] no longer holds — a card is only as current as the last re-read.
 */
internal fun blockMove(blocks: List<DocumentBlock>, block: DocumentBlock, delta: Int): PageEdit.Move? {
    val position = blocks.indexOf(block)
    if (position < 0) return null
    val neighbour = blocks.getOrNull(position + delta) ?: return null
    val to = if (delta < 0) neighbour.start else block.start + neighbour.count
    return PageEdit.Move(from = block.start, to = to, count = block.count)
}
