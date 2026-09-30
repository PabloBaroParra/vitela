package dev.vitela.pdf.core

/**
 * What adding a PDF did — pdf-ffi's `FfiImportReport`. [warnings] are the core's
 * lines for what the pages could not bring across exactly as they were (a form
 * field renamed on arrival, a link that pointed elsewhere in the source); the
 * pages are already in the document, and Undo takes them back out.
 */
data class ImportReport(val pageCount: Int, val warnings: List<String>)
