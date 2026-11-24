package dev.vitela.pdf.viewer

import dev.vitela.pdf.R
import dev.vitela.pdf.core.BlockSource
import dev.vitela.pdf.core.DocumentBlock
import dev.vitela.pdf.core.PageEdit
import dev.vitela.pdf.text.UiText

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
internal fun blockTitle(block: DocumentBlock, baseName: String, importedNames: Map<Long, String>): UiText {
    val name = when (val source = block.source) {
        BlockSource.Base -> UiText.Verbatim(baseName)
        BlockSource.Blank -> UiText.Resource(R.string.organize_block_blank)
        is BlockSource.Imported -> importedNames[source.id]?.let(UiText::Verbatim) ?: UiText.Resource(R.string.organize_block_imported)
    }
    return block.part?.let { UiText.Resource(R.string.organize_block_part, name, it) } ?: name
}

/** "7 pages · 3–9": how big the block is and where it sits now. */
internal fun blockMeta(block: DocumentBlock): UiText {
    val first = block.start + 1
    val span = if (block.count == 1) "$first" else "$first–${block.start + block.count}"
    return UiText.Plural(R.plurals.organize_block_meta, block.count, block.count, span)
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
