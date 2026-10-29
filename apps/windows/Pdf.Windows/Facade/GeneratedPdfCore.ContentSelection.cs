using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public void ApplyContentSelectionEdit(IPdfCoreDocument document, PdfCoreContentSelectionEdit edit)
    {
        var handle = ((GeneratedDocument)document).Handle;
        var selection = new FfiContentSelection(
            [.. edit.Runs.Select(ContentRun)],
            [.. edit.Images.Select(image => new FfiContentImageItem(image.Id, image.PageIndex, Rect(image.Bbox), image.ResourceXObjectName))],
            [.. edit.Paths.Select(ContentPath)]);
        try
        {
            switch (edit)
            {
                case PdfCoreContentSelectionEdit.Move move:
                    PdfFfiMethods.MoveContentSelection(handle, selection, move.Dx, move.Dy);
                    break;
                case PdfCoreContentSelectionEdit.Remove:
                    PdfFfiMethods.RemoveContentSelection(handle, selection);
                    break;
                default:
                    throw new InvalidOperationException("Unsupported selection edit.");
            }
        }
        // Like a block refusal: the core's sentence is one the reader can act on.
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, nameof(FfiException.UnsupportedOperation), refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }
}
