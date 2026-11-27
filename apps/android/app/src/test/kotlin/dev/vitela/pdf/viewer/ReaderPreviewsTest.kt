package dev.vitela.pdf.viewer

import androidx.compose.ui.graphics.ImageBitmap
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.lang.reflect.Proxy

/**
 * Which previews the reader keeps. A real render never converts to a bitmap
 * on the JVM, so these seed the state with stand-in bitmaps and drive the
 * reader directly: a preview survives a zoom, but never an edit to the pages,
 * a new document, or a scroll that leaves it outside the window.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class ReaderPreviewsTest {
    @Test
    fun redrawingThePagesDropsEveryPreview() = runTest {
        val (session, reader) = readerWith(OrganizableDocument(pageCount = 5), previews = setOf(0, 1))

        reader.pagesRedrawn(redrive = false)

        assertEquals(emptyMap<Int, ImageBitmap>(), session.state.value.previewPages)
    }

    @Test
    fun aLayoutChangeDropsEveryPreview() = runTest {
        val (session, reader) = readerWith(OrganizableDocument(pageCount = 5), previews = setOf(0, 1))

        reader.layoutChanged(redrive = false)

        assertEquals(emptyMap<Int, ImageBitmap>(), session.state.value.previewPages)
    }

    @Test
    fun aNewDocumentDropsEveryPreview() = runTest {
        val (session, reader) = readerWith(OrganizableDocument(pageCount = 5), previews = setOf(0, 1))

        reader.reset()

        assertEquals(emptyMap<Int, ImageBitmap>(), session.state.value.previewPages)
    }

    @Test
    fun aZoomKeepsThePreviews() = runTest {
        val (session, reader) = readerWith(OrganizableDocument(pageCount = 5), previews = setOf(0, 1))
        reader.onPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))
        advanceUntilIdle()

        reader.setZoom(2.0)

        assertEquals(setOf(0, 1), session.state.value.previewPages.keys)
    }

    @Test
    fun aScrollEvictsThePreviewsOutsideTheWindow() = runTest {
        val (session, reader) = readerWith(OrganizableDocument(pageCount = 100), previews = setOf(0, 90))

        reader.onPositionChanged(ReaderPosition(0, 0, 0, 1000, 1.0))

        assertEquals(setOf(0), session.state.value.previewPages.keys)
    }

    @Test
    fun whilePinchingAPageWithAPreviewWaitsForTheFingersToLift() = runTest {
        val document = OrganizableDocument(pageCount = 5)
        val (session, reader) = readerWith(document, previews = setOf(0, 1))

        reader.setZoom(0.8, live = true)
        reader.onPositionChanged(ReaderPosition(0, 1, 0, 1000, 0.8))
        advanceUntilIdle()

        val sizes = session.state.value.pageSizes
        assertTrue(document.rendered.none { (page, dpi) -> page in 0..1 && dpi == renderDpi(sizes[page], 1000, 0.8) })
        assertTrue("the page with nothing to show still renders", document.rendered.any { (page, dpi) -> page == 2 && dpi == renderDpi(sizes[page], 1000, 0.8) })
    }

    private fun TestScope.readerWith(document: OrganizableDocument, previews: Set<Int>): Pair<ViewerSession, ViewerReader> {
        val compute = StandardTestDispatcher(testScheduler)
        val initial = ViewerState(
            pageCount = document.pageCount,
            pageSizes = document.pageSizes,
            previewPages = previews.associateWith { standIn() },
        )
        val session = ViewerSession({ this }, initial, compute, compute)
        session.document = document
        return session to ViewerReader(session)
    }

    /** An [ImageBitmap] nothing draws: the reader only keeps and drops it. */
    private fun standIn(): ImageBitmap =
        Proxy.newProxyInstance(ImageBitmap::class.java.classLoader, arrayOf(ImageBitmap::class.java)) { proxy, method, args ->
            when (method.name) {
                "equals" -> proxy === args?.firstOrNull()
                "hashCode" -> System.identityHashCode(proxy)
                "toString" -> "standIn"
                else -> throw UnsupportedOperationException(method.name)
            }
        } as ImageBitmap
}
