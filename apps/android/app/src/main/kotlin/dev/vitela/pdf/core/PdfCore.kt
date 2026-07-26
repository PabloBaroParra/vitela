package dev.vitela.pdf.core

data class RenderedPage(val width: Int, val height: Int, val stride: Int, val rgba: ByteArray)

data class SearchHit(val pageIndex: Int, val text: String)

/** A page's media box, in PDF points. */
data class PageSize(val widthPt: Double, val heightPt: Double)

sealed interface PdfCoreError {
    data object PasswordRequired : PdfCoreError
    data object WrongPassword : PdfCoreError
    data class Failed(val message: String) : PdfCoreError
}

sealed interface PdfCoreResult<out T> {
    data class Success<T>(val value: T) : PdfCoreResult<T>
    data class Failure(val error: PdfCoreError) : PdfCoreResult<Nothing>
}

interface PdfDocument : AutoCloseable {
    val pageCount: Int

    /**
     * Every page's media box, in document order. The continuous reader needs
     * these up front: a page's placeholder has to be laid out at the right
     * height *before* it is rasterized, or the list resizes under the user's
     * thumb every time a render lands.
     */
    val pageSizes: List<PageSize>

    fun renderPage(pageIndex: Int, dpi: Int): PdfCoreResult<RenderedPage>
    fun search(query: String): PdfCoreResult<List<SearchHit>>
}

interface PdfCore {
    fun openFromBytes(bytes: ByteArray, password: String?): PdfCoreResult<PdfDocument>
}

/** Implemented by generated packaging sources when native bindings are present. */
interface PdfCoreFactory {
    fun create(): PdfCore
}

object PdfCoreProvider {
    fun create(): PdfCore? = ServiceLoader.load(PdfCoreFactory::class.java).firstOrNull()?.create()
}
