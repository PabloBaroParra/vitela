package dev.vitela.pdf.viewer

import dev.vitela.pdf.core.Annotation
import dev.vitela.pdf.core.AnnotationKind
import dev.vitela.pdf.core.AnnotationPoint
import dev.vitela.pdf.core.AnnotationRect
import dev.vitela.pdf.core.AnnotationSnapshot
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

/**
 * A placed image stamp — a drawn signature, a pasted or chosen image — shows
 * its picture before the file is saved, not an empty outline. The overlay can
 * only paint what the shell kept: the PNG, under the id the core gave the stamp.
 */
@OptIn(ExperimentalCoroutinesApi::class)
class StampPreviewsTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    private val existing = stamp(1)

    @Test
    fun theOneNewStampIsTheInsertedOne() {
        assertEquals(2L, insertedStampId(listOf(existing), listOf(existing, stamp(2))))
    }

    @Test
    fun aSnapshotWithNothingNewClaimsNoPreview() {
        assertNull(insertedStampId(listOf(existing), listOf(existing)))
    }

    @Test
    fun aNewAnnotationThatIsNotAStampClaimsNoPreview() {
        val note = Annotation(2, 0, AnnotationKind.TextNote, AnnotationRect(0.0, 0.0, 10.0, 10.0), null)
        assertNull(insertedStampId(listOf(existing), listOf(existing, note)))
    }

    @Test
    fun twoNewStampsAreAmbiguousAndKeepTheOutline() {
        assertNull(insertedStampId(listOf(existing), listOf(existing, stamp(2), stamp(3))))
    }

    @Test
    fun placingTheSignatureKeepsItsPictureUnderTheNewStamp() = runTest {
        val viewModel = opened()
        val png = byteArrayOf(7, 7, 7)

        place(viewModel, png)

        val placed = viewModel.state.value.annotations.single { it.kind == AnnotationKind.Stamp }
        assertArrayEquals(png, viewModel.state.value.stampImages[placed.id])
    }

    @Test
    fun placingTheSignatureRetiresItsPrompt() = runTest {
        val viewModel = opened()

        place(viewModel, byteArrayOf(1))

        assertEquals(SIGNATURE_PLACED, viewModel.state.value.status)
    }

    @Test
    fun placingAPastedImageSaysSoInItsOwnWords() = runTest {
        val viewModel = opened()
        viewModel.pasteImageStamp(byteArrayOf(1))

        viewModel.placeAnnotation(0, AnnotationPoint(20.0, 100.0), AnnotationPoint(20.0, 100.0))
        advanceUntilIdle()

        assertEquals(IMAGE_STAMP_PLACED, viewModel.state.value.status)
    }

    @Test
    fun aRefusedStampReportsTheRefusalNotThePlacement() = runTest {
        val viewModel = opened(StampingDocument(refuse = true))

        place(viewModel, byteArrayOf(1))

        assertEquals("Refused.", viewModel.state.value.status)
    }

    @Test
    fun eachStampKeepsItsOwnPicture() = runTest {
        val viewModel = opened()

        place(viewModel, byteArrayOf(1))
        place(viewModel, byteArrayOf(2))

        val (first, second) = viewModel.state.value.annotations.filter { it.kind == AnnotationKind.Stamp }
        assertArrayEquals(byteArrayOf(1), viewModel.state.value.stampImages[first.id])
        assertArrayEquals(byteArrayOf(2), viewModel.state.value.stampImages[second.id])
    }

    @Test
    fun aRefusedStampKeepsNoPicture() = runTest {
        val viewModel = opened(StampingDocument(refuse = true))

        place(viewModel, byteArrayOf(1))

        assertTrue(viewModel.state.value.stampImages.isEmpty())
    }

    @Test
    fun aReplacedDocumentStartsWithNoPictures() = runTest {
        val viewModel = opened()
        place(viewModel, byteArrayOf(1))

        viewModel.open("b.pdf", byteArrayOf(2))
        // The stamp left the document unsaved, so the replacement asks first.
        viewModel.confirmReplacement()
        viewModel.state.first { it.title == "b.pdf" && !it.isLoading }
        advanceUntilIdle()

        assertTrue(viewModel.state.value.stampImages.isEmpty())
    }

    private fun TestScope.place(viewModel: ViewerViewModel, png: ByteArray) {
        viewModel.openSignaturePad()
        advanceUntilIdle()
        viewModel.useDrawnSignature(viewModel.state.value.documentId, png)
        viewModel.placeAnnotation(0, AnnotationPoint(20.0, 100.0), AnnotationPoint(20.0, 100.0))
        advanceUntilIdle()
    }

    private suspend fun TestScope.opened(document: PdfDocument = StampingDocument()): ViewerViewModel {
        val viewModel = dispatchers.viewModel(OrganizeQueueCore(document, StampingDocument()))
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        advanceUntilIdle()
        return viewModel
    }
}

private fun stamp(id: Long) = Annotation(id, 0, AnnotationKind.Stamp, AnnotationRect(0.0, 0.0, 40.0, 20.0), null)

/** A document whose core accepts image stamps — or, with [refuse], turns every one down. */
private class StampingDocument(
    private val refuse: Boolean = false,
    private val base: PdfDocument = RetypableDocument(),
) : PdfDocument by base {
    private val stamps = mutableListOf<Annotation>()
    private var nextId = 100L

    override fun annotations() = PdfCoreResult.Success(AnnotationSnapshot(stamps.toList(), true, stamps.isNotEmpty(), false))

    override fun insertImageStamp(pageIndex: Int, imageBytes: ByteArray, rect: AnnotationRect): PdfCoreResult<Unit> {
        if (refuse) return PdfCoreResult.Failure(PdfCoreError.Failed("Refused."))
        stamps += Annotation(nextId++, pageIndex, AnnotationKind.Stamp, rect, null)
        return PdfCoreResult.Success(Unit)
    }
}
