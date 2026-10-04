package dev.vitela.pdf.viewer

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ReaderMenuTest {
    private val open = ViewerState(pageCount = 3, canOpen = true, canPrint = true)

    @Test
    fun theMenuListsEveryDocumentActionInItsGroups() {
        assertEquals(
            listOf(
                DocumentMenuItem.Open, DocumentMenuItem.Save, DocumentMenuItem.SaveCopy, DocumentMenuItem.Print,
                DocumentMenuItem.Compress, DocumentMenuItem.Protect, DocumentMenuItem.ExportImages,
                DocumentMenuItem.ExtractPages, DocumentMenuItem.Split,
                DocumentMenuItem.PreviousAnnotation, DocumentMenuItem.NextAnnotation, DocumentMenuItem.Properties,
            ),
            DocumentMenuItem.entries,
        )
    }

    @Test
    fun saveInPlaceNeedsChangesAndAFileToWriteBack() {
        assertFalse(DocumentMenuItem.Save.enabled(open))
        assertFalse(DocumentMenuItem.Save.enabled(open.copy(isDirty = true)))
        assertTrue(DocumentMenuItem.Save.enabled(open.copy(isDirty = true, saveTarget = "content://doc")))
        assertTrue(DocumentMenuItem.SaveCopy.enabled(open.copy(isDirty = true)))
    }

    @Test
    fun aRunningJobDisablesOnlyItsOwnEntry() {
        val compressing = open.copy(compressRunning = true)

        assertFalse(DocumentMenuItem.Compress.enabled(compressing))
        assertTrue(DocumentMenuItem.Protect.enabled(compressing))
    }

    @Test
    fun splitNeedsTwoPages() {
        assertFalse(DocumentMenuItem.Split.enabled(open.copy(pageCount = 1)))
        assertTrue(DocumentMenuItem.Split.enabled(open))
    }

    @Test
    fun annotationStepsNeedAnnotations() {
        assertFalse(DocumentMenuItem.NextAnnotation.enabled(open))
    }

    @Test
    fun groupsStartAtCompressAndAtTheAnnotationSteps() {
        assertEquals(
            listOf(DocumentMenuItem.Compress, DocumentMenuItem.PreviousAnnotation),
            DocumentMenuItem.entries.filter { it.startsGroup },
        )
    }
}
