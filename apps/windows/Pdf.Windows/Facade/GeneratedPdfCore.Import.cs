using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public IImportSource PrepareImport(byte[] bytes, string? password)
    {
        try { return new Source(PdfFfiMethods.PrepareImport(bytes, password)); }
        catch (FfiException error) { throw TranslateImport(error); }
    }

    public string? ImportRefusalOf(IPdfCoreDocument document) =>
        PdfFfiMethods.ImportRefusal(((GeneratedDocument)document).Handle);

    public PdfCoreBatchImportReport ImportPrepared(IPdfCoreDocument document, IReadOnlyList<IImportSource> sources, uint index)
    {
        try
        {
            var report = PdfFfiMethods.ImportPrepared(((GeneratedDocument)document).Handle,
                [.. sources.Select(source => ((Source)source).Handle)], index);
            return new PdfCoreBatchImportReport(report.PageCount, report.SourceIds);
        }
        catch (FfiException error) { throw TranslateImport(error); }
    }

    // Every import refusal is a sentence written for the reader (see pdf-ffi's
    // import.rs), so here — unlike Translate's other categories — the detail crosses.
    private static PdfCoreException TranslateImport(FfiException error) => error is FfiException.UnsupportedOperation refusal
        ? new PdfCoreException(PdfCoreError.UnsupportedOperation, refusal.GetType().Name, refusal.detail)
        : Translate(error);

    private sealed class Source(PreparedImport handle) : IImportSource
    {
        internal PreparedImport Handle { get; } = handle;
        public uint PageCount { get; } = handle.PageCount();
        public IReadOnlyList<string> Warnings { get; } = handle.Warnings();
        public void Dispose() => Handle.Dispose();
    }
}
