using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    // Paragraphs, not lines: this shell's editor is multi-line, as Linux's
    // is. The core still reads a composite-font paragraph line by line.
    public IReadOnlyList<PdfCoreTextBlock> ReadPageParagraphBlocks(IPdfCoreDocument document, uint pageIndex)
    {
        try
        {
            return [.. PdfFfiMethods.ReadPageParagraphBlocks(((GeneratedDocument)document).Handle, pageIndex)
                .Select(block => new PdfCoreTextBlock([.. block.Runs.Select(run => run.Id)], Rect(block.Bbox), block.Text,
                    Math.Max(1, block.Lines.Length), block.Leading))];
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public void ApplyTextBlockEdit(IPdfCoreDocument document, PdfCoreTextBlockEdit edit)
    {
        var handle = ((GeneratedDocument)document).Handle;
        var runs = edit.Runs.Select(ContentRun).ToArray();
        try
        {
            switch (edit)
            {
                case PdfCoreTextBlockEdit.Move move:
                    PdfFfiMethods.MoveTextBlock(handle, runs, move.Dx, move.Dy);
                    break;
                case PdfCoreTextBlockEdit.Remove:
                    PdfFfiMethods.RemoveTextBlock(handle, runs);
                    break;
                case PdfCoreTextBlockEdit.Retype retype:
                    PdfFfiMethods.RetypeTextBlock(handle, runs, retype.Text, retype.WithInsertedFont);
                    break;
                default:
                    throw new InvalidOperationException("Unsupported text block edit.");
            }
        }
        // A block refusal is the core's own sentence for the reader — "save
        // the document before editing it again" is something they can act on,
        // and the generic "not supported" would read as a bug.
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, nameof(FfiException.UnsupportedOperation), refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }
}
