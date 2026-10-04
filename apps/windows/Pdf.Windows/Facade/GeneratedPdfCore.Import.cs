using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public PdfCoreImportReport ImportPdf(IPdfCoreDocument document, byte[] bytes, string? password, uint index)
    {
        try
        {
            var report = PdfFfiMethods.ImportPdf(((GeneratedDocument)document).Handle, bytes, password, index);
            return new PdfCoreImportReport(report.PageCount, report.Warnings, report.SourceId);
        }
        // Every import refusal is a sentence written for the reader (see
        // pdf-ffi's import.rs), so here — unlike Translate's other
        // categories — the detail crosses.
        catch (FfiException.UnsupportedOperation refusal)
        {
            throw new PdfCoreException(PdfCoreError.UnsupportedOperation, refusal.GetType().Name, refusal.detail);
        }
        catch (FfiException error) { throw Translate(error); }
    }
}
