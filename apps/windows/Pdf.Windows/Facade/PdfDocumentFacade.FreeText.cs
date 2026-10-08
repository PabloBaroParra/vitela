namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>
    /// The core's layout of a FreeText box's text, for the moments no stored box
    /// exists to read lines from: the dialog previewing what is typed and a
    /// resize drag rewrapping live. Takes no session and no lock — it is pure
    /// text measurement, the same shape as <see cref="PlaceRect"/>.
    /// A character the font cannot show fails with the message that names it.
    /// </summary>
    public OperationResult<FreeTextLayout> LayoutFreeText(string contents, double widthPt, double heightPt)
    {
        try
        {
            return OperationResult<FreeTextLayout>.Success(_core.LayoutFreeText(contents, widthPt, heightPt));
        }
        catch (PdfCoreException error)
        {
            return OperationResult<FreeTextLayout>.Failure(MapError(error, "freetext_layout", null, null));
        }
    }
}
