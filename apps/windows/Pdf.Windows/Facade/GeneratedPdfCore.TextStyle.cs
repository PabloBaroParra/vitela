using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public TextRunLookState? ReadTextRunLook(IPdfCoreDocument document, PdfCoreContentTextRun run)
    {
        try
        {
            return PdfFfiMethods.TextRunLook(((GeneratedDocument)document).Handle, ContentRun(run)) is { } state
                ? new TextRunLookState(Look(state.Look), state.CanRemoveBold, state.CanRemoveItalic)
                : null;
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public void RestyleTextRuns(IPdfCoreDocument document, IReadOnlyList<PdfCoreTextRunRestyle> restyles)
    {
        try
        {
            PdfFfiMethods.RestyleTextRuns(((GeneratedDocument)document).Handle,
                [.. restyles.Select(restyle => new FfiTextRunRestyle(ContentRun(restyle.Item), Look(restyle.From), Look(restyle.To)))]);
        }
        // The core's refusal is a sentence the reader can act on ("its font
        // has no plain sibling", "save the document before editing it
        // again"), as for a text block.
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, nameof(FfiException.UnsupportedOperation), refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }

    private static TextRunLook Look(FfiTextRunLook look) => new(look.Bold, look.Italic, look.Underline, look.Strike);

    private static FfiTextRunLook Look(TextRunLook look) => new(look.Bold, look.Italic, look.Underline, look.Strike);
}
