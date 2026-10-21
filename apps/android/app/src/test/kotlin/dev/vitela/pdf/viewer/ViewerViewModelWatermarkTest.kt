package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.PictureWatermarkLook
import dev.vitela.pdf.core.WatermarkLayer
import dev.vitela.pdf.core.WatermarkLook
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/** Add watermark: one dialog, one undoable stamp over the pages picked, then the pages redrawn. */
class ViewerViewModelWatermarkTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    @Test
    fun openingTheDialogStartsFromTheCoresLook() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()

        val editor = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        assertEquals(WatermarkDefaults(document.look, document.pictureLook), editor.defaults)
        assertEquals(editor.defaults.draft(), editor.draft)
        assertEquals("DRAFT", editor.draft.text)
        assertEquals(WatermarkPages.All, editor.draft.pages)
        assertNull(editor.message)
    }

    @Test
    fun aDocumentWithoutTheModifyBitSaysSoInsteadOfOpening() = runTest {
        val viewModel = openedWith(WatermarkDocument(editingAllowed = false))
        viewModel.openWatermark()

        val state = viewModel.state.first { it.status == CONTENT_EDIT_FORBIDDEN }
        assertNull(state.watermark)
    }

    @Test
    fun applyingStampsThePagesAsOneUndoableEditAndRedraws() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        val look = WatermarkLook(fontSizePt = 40.0, opacity = 0.5, rotationDegrees = -30.0, layer = WatermarkLayer.UnderContent)
        viewModel.editWatermark(WatermarkDefaults(look, document.pictureLook).draft().copy(text = "COPY", pages = WatermarkPages.Custom, range = "1,3"))
        val refreshesBefore = document.previewRefreshes
        viewModel.applyWatermark()

        val state = viewModel.state.first { it.status == "Watermark added to 2 pages." }
        assertNull(state.watermark)
        assertEquals(listOf(Triple(listOf(0, 2), "COPY", look)), document.stamps)
        assertTrue(state.isDirty)
        assertEquals(1L, state.revision)
        assertTrue(state.canUndoAnnotations)
        // The stamp is page content: the pages only show it from a rebuilt preview.
        assertTrue(document.previewRefreshes > refreshesBefore)
    }

    @Test
    fun theCurrentPageIsTheOneTheReaderIsOn() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        advanceUntilIdle()
        viewModel.onReaderPositionChanged(ReaderPosition(1, 1, 1, 1000, 1.0))
        advanceUntilIdle()
        viewModel.openWatermark()
        val editor = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        assertEquals(1, editor.currentPage)
        viewModel.editWatermark(editor.draft.copy(pages = WatermarkPages.Current))
        viewModel.applyWatermark()

        viewModel.state.first { it.status == "Watermark added to 1 page." }
        assertEquals(listOf(1), document.stamps.single().first)
    }

    @Test
    fun aBadRangeKeepsTheDialogOpenWithTheReasonAndWhatWasTyped() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        val opened = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        val typed = opened.draft.copy(pages = WatermarkPages.Custom, range = "9")
        viewModel.editWatermark(typed)
        viewModel.applyWatermark()

        val editor = requireNotNull(viewModel.state.first { it.watermark?.message != null }.watermark)
        assertEquals("There is no page 9.", editor.message)
        assertEquals(typed, editor.draft)
        assertEquals(emptyList<Any>(), document.stamps)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aCoreRefusalKeepsTheDialogOpenWithTheCoresSentence() = runTest {
        val document = WatermarkDocument(refusal = "The watermark font cannot show “€”. Use Latin letters, digits and punctuation.")
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        viewModel.applyWatermark()

        val editor = requireNotNull(viewModel.state.first { it.watermark?.message != null }.watermark)
        assertEquals("The watermark font cannot show “€”. Use Latin letters, digits and punctuation.", editor.message)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun aSecondTapOnAddStampsOnce() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        viewModel.applyWatermark()
        viewModel.applyWatermark()

        viewModel.state.first { it.status == "Watermark added to 3 pages." }
        assertEquals(1, document.stamps.size)
    }

    @Test
    fun aChosenPictureLandsOnTheDraftAndClearsAnEarlierRefusal() = runTest {
        val viewModel = openedWith(WatermarkDocument())
        viewModel.openWatermark()
        val opened = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        viewModel.editWatermark(opened.switchedTo(WatermarkKind.Picture))
        viewModel.applyWatermark()
        viewModel.state.first { it.watermark?.message != null }

        val bytes = byteArrayOf(9, 8, 7)
        viewModel.chooseWatermarkPicture(bytes)

        val editor = requireNotNull(viewModel.state.value.watermark)
        assertSame(bytes, editor.draft.picture?.bytes)
        assertNull(editor.message)
    }

    @Test
    fun anUnreadablePictureSaysSoInTheDialog() = runTest {
        val viewModel = openedWith(WatermarkDocument())
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        viewModel.watermarkPictureUnreadable()

        assertEquals(IMAGE_UNREADABLE, viewModel.state.value.watermark?.message)
        assertNull(viewModel.state.value.watermark?.draft?.picture)
    }

    @Test
    fun aPictureArrivingAfterTheDialogClosedIsDropped() = runTest {
        val viewModel = openedWith(WatermarkDocument())
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        viewModel.dismissWatermark()
        viewModel.chooseWatermarkPicture(byteArrayOf(1))

        assertNull(viewModel.state.value.watermark)
    }

    @Test
    fun applyingAPictureStampsItsBytesUntouchedAsOneUndoableEdit() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        val opened = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        val bytes = byteArrayOf(0x89.toByte(), 0x50, 0x4E, 0x47)
        viewModel.chooseWatermarkPicture(bytes)
        val picked = requireNotNull(viewModel.state.value.watermark).draft
        viewModel.editWatermark(picked.switchedTo(WatermarkKind.Picture, opened.defaults).copy(scalePercent = 80.0, opacityPercent = 40.0, pages = WatermarkPages.Current))
        val refreshesBefore = document.previewRefreshes
        viewModel.applyWatermark()

        val state = viewModel.state.first { it.status == "Watermark added to 1 page." }
        assertNull(state.watermark)
        assertEquals(emptyList<Any>(), document.stamps)
        val (pages, image, look) = document.pictureStamps.single()
        assertEquals(listOf(0), pages)
        assertSame(bytes, image)
        assertEquals(PictureWatermarkLook(scale = 0.8, opacity = 0.4, rotationDegrees = 0.0, layer = WatermarkLayer.UnderContent), look)
        assertTrue(state.isDirty)
        assertTrue(state.canUndoAnnotations)
        assertTrue(document.previewRefreshes > refreshesBefore)
    }

    @Test
    fun aRefusedPictureKeepsTheDialogOpenWithTheCoresSentence() = runTest {
        val sentence = "That file could not be read as a PNG or JPEG picture. Choose another one."
        val viewModel = openedWith(WatermarkDocument(refusal = sentence))
        viewModel.openWatermark()
        val opened = requireNotNull(viewModel.state.first { it.watermark != null }.watermark)
        viewModel.editWatermark(opened.switchedTo(WatermarkKind.Picture).copy(picture = WatermarkPicture(byteArrayOf(1))))
        viewModel.applyWatermark()

        val editor = requireNotNull(viewModel.state.first { it.watermark?.message != null }.watermark)
        assertEquals(sentence, editor.message)
        assertEquals(WatermarkKind.Picture, editor.draft.kind)
        assertFalse(viewModel.state.value.isDirty)
    }

    @Test
    fun dismissingDropsTheDraftAndStampsNothing() = runTest {
        val document = WatermarkDocument()
        val viewModel = openedWith(document)
        viewModel.openWatermark()
        viewModel.state.first { it.watermark != null }
        viewModel.dismissWatermark()

        assertNull(viewModel.state.value.watermark)
        assertEquals(emptyList<Any>(), document.stamps)
    }

    private suspend fun openedWith(document: WatermarkDocument): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}
