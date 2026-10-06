package dev.vitela.pdf.core

import org.junit.Assert.assertEquals
import org.junit.Test

class CoreFailureTest {
    private fun message(failure: CoreFailure, detail: String = ""): String =
        (failure.toPdfCoreError(detail) as PdfCoreError.Failed).message

    @Test
    fun passwordFailuresKeepTheirOwnErrors() {
        assertEquals(PdfCoreError.PasswordRequired, CoreFailure.PasswordRequired.toPdfCoreError(""))
        assertEquals(PdfCoreError.WrongPassword, CoreFailure.WrongPassword.toPdfCoreError(""))
    }

    @Test
    fun aRenderFailureIsNotReportedAsAMissingLibrary() {
        assertEquals("This page could not be rendered.", message(CoreFailure.RenderFailed, "render failed: bad xref"))
    }

    @Test
    fun aMissingPdfiumLibraryIsStillNamed() {
        val detail = "failed to load pdfium library at \"libpdfium.so\": not found. Set PDFIUM_DYNAMIC_LIB_PATH"
        assertEquals(PDFIUM_UNAVAILABLE, message(CoreFailure.RenderFailed, detail))
    }

    @Test
    fun aStaleIdReadsAsTheDocumentChanging() {
        for (failure in listOf(CoreFailure.PageIndexOutOfBounds, CoreFailure.AnnotationNotFound, CoreFailure.FormFieldNotFound, CoreFailure.BitmapNotFound)) {
            assertEquals("The document changed. Please try again.", message(failure))
        }
    }

    @Test
    fun theReaderFacingDetailsCrossAsTyped() {
        assertEquals("This text's font cannot show \"€\". Try different characters.", message(CoreFailure.EncodingGap, "€"))
        assertEquals("Page 9 is past the end.", message(CoreFailure.InvalidPageSelection, "Page 9 is past the end."))
    }

    @Test
    fun theRemainingKindsAreNamedApart() {
        assertEquals("This document uses a password protection this app does not support.", message(CoreFailure.UnsupportedSecurityHandler))
        assertEquals("This document or action is not supported.", message(CoreFailure.UnsupportedOperation))
        assertEquals(NOT_AN_IMAGE, message(CoreFailure.InvalidImage))
        assertEquals("The requested action could not be completed.", message(CoreFailure.InvalidSaveRequest))
        assertEquals("Saving would invalidate this document's signature, so it was not saved.", message(CoreFailure.SignaturesWouldBeInvalidated))
        assertEquals("The file could not be read or written.", message(CoreFailure.Io))
    }

    @Test
    fun onlyAnUnmodelledFailureFallsBackToTheGenericSentence() {
        assertEquals(GENERIC_FAILURE, message(CoreFailure.Internal, "internal error: lopdf parse"))
        assertEquals(GENERIC_FAILURE, message(CoreFailure.Unexpected))
    }
}
