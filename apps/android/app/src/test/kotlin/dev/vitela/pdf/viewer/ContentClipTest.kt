package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.ContentFontKind
import dev.vitela.pdf.core.ContentImage
import dev.vitela.pdf.core.ContentTextBlock
import dev.vitela.pdf.core.ContentTextRun
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** Telling what a paste added to a page, from two reads of it (B25 T-209). */
class ContentClipTest {
    @Test
    fun pastedItemsAreFoundAsTheOnesThePageDidNotHaveBefore() {
        val before = PageIds(runs = setOf(1L, 2L), images = setOf(5L), paths = emptySet())
        val hello = textBlock(7L, 1, "Hello")
        val old = textBlock(1L, 1, "Old")
        val image = ContentImage(5, 1, AnnotationRect(0.0, 0.0, 1.0, 1.0), null)
        val newImage = image.copy(id = 9)

        val picked = selectionOfPaste(1, before, listOf(old, hello), listOf(image, newImage), emptyList())

        assertEquals(listOf(7L), picked?.blocks?.map { it.id })
        assertEquals(listOf(9L), picked?.images?.map { it.id })
        assertNull(selectionOfPaste(1, before, listOf(old), listOf(image), emptyList()))
    }

    @Test
    fun aBlockThatMergedWithAnOldRunStillCountsWhenAnyRunIsNew() {
        val before = PageIds(runs = setOf(1L), images = emptySet(), paths = emptySet())
        val merged = ContentTextBlock(listOf(run(1L, 1, "Old"), run(7L, 1, "New")), AnnotationRect(0.0, 0.0, 10.0, 10.0), "Old New")

        assertEquals(listOf(1L), selectionOfPaste(1, before, listOf(merged), emptyList(), emptyList())?.blocks?.map { it.id })
    }

    private fun run(id: Long, page: Int, text: String) =
        ContentTextRun(id, page, AnnotationRect(0.0, 0.0, 5.0, 5.0), "F1", ContentFontKind.Standard14, text)

    private fun textBlock(id: Long, page: Int, text: String) =
        ContentTextBlock(listOf(run(id, page, text)), AnnotationRect(0.0, 0.0, 5.0, 5.0), text)
}
