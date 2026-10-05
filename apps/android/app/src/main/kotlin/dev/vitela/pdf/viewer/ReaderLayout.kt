package dev.vitela.pdf.viewer

/** Window-width decisions, independent of device identity and orientation. */
internal data class ReaderLayout(val sideTools: Boolean, val canShowSpread: Boolean)

internal fun readerLayout(widthDp: Float) = ReaderLayout(
    sideTools = widthDp >= 600f,
    canShowSpread = widthDp >= 1000f,
)

/** Maps lazy-list rows to document pages without manufacturing a final blank page. */
internal class PageRows(val pageCount: Int, val columns: Int) {
    init {
        require(pageCount >= 0)
        require(columns in 1..2)
    }

    val count: Int get() = (pageCount + columns - 1) / columns
    fun row(page: Int): Int = boundedPageIndex(page, pageCount) / columns
    fun first(row: Int): Int = row * columns
    fun last(row: Int): Int = minOf(first(row) + columns - 1, pageCount - 1)
    fun current(row: Int, preferred: Int): Int = preferred.takeIf { it in first(row)..last(row) } ?: first(row)
}
