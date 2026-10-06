package dev.vitela.pdf.core

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

const val GENERIC_FAILURE = "The document could not be processed."

/** Only PNG and JPEG decode; the picker cannot be narrowed to them on every device. */
const val NOT_AN_IMAGE = "This file is not a PNG or JPEG image."

const val PDFIUM_UNAVAILABLE = "PDFium is unavailable. This package must include externally supplied compatible PDFium libraries."

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

private fun CoreFailure.message(detail: String): String = when (this) {
    CoreFailure.PasswordRequired, CoreFailure.WrongPassword -> "This document requires a password."
    CoreFailure.UnsupportedSecurityHandler -> "This document uses a password protection this app does not support."
    CoreFailure.DocumentNotFound,
    CoreFailure.BitmapNotFound,
    CoreFailure.PageIndexOutOfBounds,
    CoreFailure.AnnotationNotFound,
    CoreFailure.FormFieldNotFound -> "The document changed. Please try again."
    CoreFailure.InvalidImage -> NOT_AN_IMAGE
    CoreFailure.InvalidSaveRequest -> "The requested action could not be completed."
    CoreFailure.SignaturesWouldBeInvalidated -> "Saving would invalidate this document's signature, so it was not saved."
    CoreFailure.UnsupportedOperation -> "This document or action is not supported."
    CoreFailure.EncodingGap -> "This text's font cannot show \"$detail\". Try different characters."
    CoreFailure.InvalidPageSelection -> detail
    CoreFailure.RenderFailed -> if (detail.contains(LIBRARY_LOAD_PREFIX)) PDFIUM_UNAVAILABLE else "This page could not be rendered."
    CoreFailure.Io -> "The file could not be read or written."
    CoreFailure.Internal, CoreFailure.Unexpected -> GENERIC_FAILURE
}
