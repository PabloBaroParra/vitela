package dev.vitela.pdf.core

/** The encodings a page can be exported as. JPEG has no alpha: the core flattens the page onto an opaque background. */
enum class ImageExportFormat(val label: String, val mimeType: String) {
    Png("PNG", "image/png"),
    Jpeg("JPEG", "image/jpeg"),
}
