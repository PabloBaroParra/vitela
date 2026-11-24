package dev.vitela.pdf.core

import dev.vitela.pdf.R
import dev.vitela.pdf.text.UiText

/**
 * What a core call failed with, one entry per `FfiError` variant — named here,
 * outside the generated adapter, so each one's sentence is tested without the
 * native bindings. [Unexpected] is anything that is not an `FfiError` at all:
 * a Rust panic crossing as UniFFI's internal error, or a bug on this side.
 *
 * Only [Internal] and [Unexpected] fall back to [GENERIC_FAILURE]. Every other
 * kind is something the reader can tell apart, and folding it into "could not
 * be processed" hid which one it was.
 */
enum class CoreFailure {
    PasswordRequired,
    WrongPassword,
    UnsupportedSecurityHandler,
    DocumentNotFound,
    BitmapNotFound,
    PageIndexOutOfBounds,
    AnnotationNotFound,
    FormFieldNotFound,
    InvalidImage,
    InvalidSaveRequest,
    SignaturesWouldBeInvalidated,
    UnsupportedOperation,
    EncodingGap,
    InvalidPageSelection,
    RenderFailed,
    Io,
    Internal,
    Unexpected,
}

val GENERIC_FAILURE: UiText = UiText.Resource(R.string.error_core_generic)

/** Only PNG and JPEG decode; the picker cannot be narrowed to them on every device. */
val NOT_AN_IMAGE: UiText = UiText.Resource(R.string.error_core_not_an_image)

val PDFIUM_UNAVAILABLE: UiText = UiText.Resource(R.string.error_core_pdfium_unavailable)

/**
 * `pdf_render::RenderError::LibraryLoad`'s Display. The FFI folds it into
 * `RenderFailed` with every other render failure, so its wording is the only
 * thing that tells a missing library apart from a page PDFium could not draw.
 */
private const val LIBRARY_LOAD_PREFIX = "failed to load pdfium library"

/**
 * [detail] is the variant's own field: the character for [CoreFailure.EncodingGap],
 * the core's sentence for [CoreFailure.InvalidPageSelection], the diagnostic
 * clause otherwise. Only those first two reach the reader as they are.
 */
fun CoreFailure.toPdfCoreError(detail: String): PdfCoreError = when (this) {
    CoreFailure.PasswordRequired -> PdfCoreError.PasswordRequired
    CoreFailure.WrongPassword -> PdfCoreError.WrongPassword
    else -> PdfCoreError.Failed(message(detail))
}

private fun CoreFailure.message(detail: String): UiText = when (this) {
    CoreFailure.PasswordRequired, CoreFailure.WrongPassword -> UiText.Resource(R.string.status_open_password_required)
    CoreFailure.UnsupportedSecurityHandler -> UiText.Resource(R.string.error_core_security_handler)
    CoreFailure.DocumentNotFound,
    CoreFailure.BitmapNotFound,
    CoreFailure.PageIndexOutOfBounds,
    CoreFailure.AnnotationNotFound,
    CoreFailure.FormFieldNotFound -> UiText.Resource(R.string.error_core_document_changed)
    CoreFailure.InvalidImage -> NOT_AN_IMAGE
    CoreFailure.InvalidSaveRequest -> UiText.Resource(R.string.error_core_request_failed)
    CoreFailure.SignaturesWouldBeInvalidated -> UiText.Resource(R.string.error_core_signatures_invalidated)
    CoreFailure.UnsupportedOperation -> UiText.Resource(R.string.error_core_unsupported)
    CoreFailure.EncodingGap -> UiText.Resource(R.string.error_core_font_gap, detail)
    CoreFailure.InvalidPageSelection -> UiText.Verbatim(detail)
    CoreFailure.RenderFailed -> if (detail.contains(LIBRARY_LOAD_PREFIX)) PDFIUM_UNAVAILABLE else UiText.Resource(R.string.error_core_page_not_rendered)
    CoreFailure.Io -> UiText.Resource(R.string.error_core_io)
    CoreFailure.Internal, CoreFailure.Unexpected -> GENERIC_FAILURE
}

/**
 * A refusal the core words as a lower-case clause, as the sentence it stands
 * for. Verbatim: the core's wording has no translation.
 */
fun coreSentence(clause: String): UiText = UiText.Verbatim("${clause.replaceFirstChar { it.uppercase() }.trimEnd('.')}.")
