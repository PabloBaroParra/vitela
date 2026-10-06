package dev.vitela.pdf.core

/**
 * One PDF opened and checked for import — pdf-ffi's `PreparedImport`. Holding
 * it changes nothing about the open document; it holds the decrypted source
 * until [PdfDocument.importPrepared] takes it over or [close] drops it, which
 * is how an import is abandoned. [warnings] are the core's lines for what the
 * pages will not bring across exactly as they are (a form field renamed on
 * arrival, a link that points elsewhere in the source), known before anything
 * is added.
 */
interface PreparedImport : AutoCloseable {
    val pageCount: Int
    val warnings: List<String>
}

/**
 * What [PdfDocument.importPrepared] did — pdf-ffi's `FfiBatchImportReport`.
 * [sourceIds] are the ids the added blocks carry ([BlockSource.Imported]), one
 * per source, in the order the sources were given.
 */
data class BatchImportReport(val pageCount: Int, val sourceIds: List<Long>)
