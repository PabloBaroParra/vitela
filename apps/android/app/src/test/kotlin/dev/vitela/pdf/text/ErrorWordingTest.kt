package dev.vitela.pdf.text

import dev.vitela.pdf.R
import dev.vitela.pdf.core.CompressedCopy
import dev.vitela.pdf.core.CoreFailure
import dev.vitela.pdf.core.PageSize
import dev.vitela.pdf.core.PdfCore
import dev.vitela.pdf.core.PdfCoreError
import dev.vitela.pdf.core.PdfCoreResult
import dev.vitela.pdf.core.PdfDocument
import dev.vitela.pdf.core.RenderedPage
import dev.vitela.pdf.core.SearchHit
import dev.vitela.pdf.core.coreSentence
import dev.vitela.pdf.core.toPdfCoreError
import dev.vitela.pdf.resources.StringResources
import dev.vitela.pdf.viewer.ViewerDispatcherRule
import dev.vitela.pdf.viewer.ViewerViewModel
import dev.vitela.pdf.viewer.compressNoGainSummary
import dev.vitela.pdf.viewer.userMessage
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import java.io.File

/** What went wrong, in the reader's language wherever it is shown. */
class ErrorWordingTest {
    @get:Rule
    val dispatchers = ViewerDispatcherRule()

    /** One published value words both languages, so a change of language words it again. */
    @Test
    fun aFailureIsWordedInWhicheverLanguageIsInUse() {
        val shown = userMessage(CoreFailure.Io.toPdfCoreError("disk full"))
        assertEquals("The file could not be read or written.", shown.english())
        assertEquals("No se pudo leer o escribir el archivo.", shown.spanish())
        assertEquals("The file could not be read or written.", shown.english())
    }

    @Test
    fun aFailureInTheStatusLineIsTranslated() = runTest {
        val viewModel = opened()
        val before = viewModel.state.value.status
        viewModel.openMetadata()

        val status = viewModel.state.first { it.status != before }.status
        assertEquals("Document properties are unavailable in this PDF core.", status.english())
        assertEquals("Las propiedades del documento no están disponibles en este núcleo PDF.", status.spanish())
    }

    @Test
    fun aRefusalInADialogIsTranslated() = runTest {
        val viewModel = opened()
        viewModel.openProtect()

        val refusal = requireNotNull(viewModel.state.first { it.protect != null }.protect).refusal
        assertEquals("This document does not permit protection changes.", refusal?.english())
        assertEquals("Este documento no permite cambiar su protección.", refusal?.spanish())
    }

    @Test
    fun whatTheCoreDoesNotOfferIsTranslatedInADialog() = runTest {
        val viewModel = opened()
        viewModel.openCompress()

        val refusal = requireNotNull(viewModel.state.first { it.compress != null }.compress).refusal
        assertEquals("Compression is unavailable in this PDF core.", refusal?.english())
        assertEquals("La compresión no está disponible en este núcleo PDF.", refusal?.spanish())
    }

    @Test
    fun aFailureInsideASentenceIsWordedInTheSameLanguage() {
        val failure = userMessage(PdfCoreError.Failed(R.string.error_export_not_rendered, 400))
        val text = UiText.Resource(R.string.status_export_page_failed, 3, failure, 0)
        assertEquals("Page 3 could not be exported: It could not be rendered at 400 DPI. Choose a lower resolution. 0 written.", text.english())
        assertEquals("No se pudo exportar la página 3: No se pudo generar a 400 DPI. Elige una resolución más baja. Imágenes escritas: 0.", text.spanish())
    }

    @Test
    fun theCoresOwnWordsStayInEnglishInsideATranslatedSentence() {
        val text = UiText.Resource(R.string.error_extract_failed, "the page tree is broken")
        assertEquals("These pages could not be extracted: the page tree is broken.", text.english())
        assertEquals("No se pudieron extraer estas páginas: the page tree is broken.", text.spanish())
        assertEquals("Save the document first.", coreSentence("save the document first").spanish())
    }

    @Test
    fun eachCompressRefusalIsWordedInTheListsLanguage() {
        val refusals = listOf(UiText.Resource(R.string.error_compress_signed_clause), UiText.Verbatim("out of memory"))
        val copy = CompressedCopy(ByteArray(0), 40_000, 40_000, 0, false, refusals)
        val english = compressNoGainSummary(copy).english()
        val spanish = compressNoGainSummary(copy).spanish()
        assertTrue(english, english.endsWith("Not everything could be done: compressing rewrites the file and would stop its signature from verifying; out of memory."))
        assertTrue(spanish, spanish.endsWith("No se pudo hacer todo: comprimir reescribe el archivo y su firma dejaría de ser válida; out of memory."))
    }

    @Test
    fun everyErrorSentenceWithArgumentsIsWorded() {
        val sentences = listOf(
            UiText.Resource(R.string.error_core_font_gap, "€") to "This text's font cannot show \"€\". Try different characters.",
            UiText.Resource(R.string.error_content_helvetica_gap, "€") to "Helvetica cannot show \"€\". Try different characters.",
            UiText.Resource(R.string.error_watermark_font_gap, "€") to "The watermark font cannot show “€”. Use Latin letters, digits and punctuation.",
            UiText.Resource(R.string.error_export_dpi_range, 72, 400) to "Choose a resolution between 72 and 400 DPI.",
            UiText.Resource(R.string.error_export_page_too_large, 3, 400) to "Page 3 is too large to export at 400 DPI. Choose a lower resolution.",
        )
        sentences.forEach { (text, english) ->
            assertEquals(english, text.english())
            assertNotEquals(english, text.spanish())
        }
    }

    /**
     * The adapter is compiled only when the native core is packaged, so a JVM
     * build would not notice a sentence worded in it, or a name that is gone.
     */
    @Test
    fun theAdapterNamesItsSentencesInsteadOfWordingThem() {
        val template = File("../GeneratedPdfCore.kt.template").readText()
        val worded = Regex("""(Failed|Verbatim|Refusal|authorField)\([^)\n]*"""").find(template)
        assertEquals(null, worded?.value)

        val named = Regex("""R\.string\.(\w+)""").findAll(template).map { it.groupValues[1] }.toSet()
        assertFalse(named.isEmpty())
        assertEquals(emptySet<String>(), named - StringResources.english.strings.keys)
    }

    private suspend fun opened(): ViewerViewModel {
        val viewModel = dispatchers.viewModel(BareCore())
        viewModel.open("a.pdf", byteArrayOf(1))
        viewModel.state.first { it.documentId != 0L && !it.isLoading }
        return viewModel
    }
}

/** A core that offers nothing beyond the required: every other answer is the interface's own. */
private class BareCore : PdfCore {
    override fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument> = PdfCoreResult.Success(BareDocument())
}

private class BareDocument : PdfDocument {
    override val pageCount = 1
    override val pageSizes = listOf(PageSize(612.0, 792.0))
    override fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage> = PdfCoreResult.Success(RenderedPage(1, 1, 4, ByteArray(4)))
    override fun search(query: String): PdfCoreResult<List<SearchHit>> = PdfCoreResult.Success(emptyList())
    override fun close() = Unit
}
