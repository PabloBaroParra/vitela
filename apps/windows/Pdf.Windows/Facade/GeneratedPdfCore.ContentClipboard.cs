using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public PdfCoreContentClip CopyContentSelection(IPdfCoreDocument document, PdfCoreContentSelectionEdit.Remove members, bool cut)
    {
        var handle = ((GeneratedDocument)document).Handle;
        var selection = new FfiContentSelection(
            [.. members.Runs.Select(ContentRun)],
            [.. members.Images.Select(image => new FfiContentImageItem(image.Id, image.PageIndex, Rect(image.Bbox), image.ResourceXObjectName))],
            [.. members.Paths.Select(ContentPath)]);
        try
        {
            var clip = cut
                ? PdfFfiMethods.CutContentSelection(handle, selection)
                : PdfFfiMethods.CopyContentSelection(handle, selection);
            return new PdfCoreContentClip(clip.Pdf, Rect(clip.Bbox));
        }
        // Like a selection refusal: the core's sentence is one the reader can act on.
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, nameof(FfiException.UnsupportedOperation), refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public void PasteContent(IPdfCoreDocument document, uint pageIndex, PdfCoreContentClip clip, double left, double top)
    {
        try
        {
            PdfFfiMethods.PasteContent(((GeneratedDocument)document).Handle, pageIndex,
                new FfiContentClip(clip.Pdf, Rect(clip.Bbox)), left, top);
        }
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, nameof(FfiException.UnsupportedOperation), refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }
}
