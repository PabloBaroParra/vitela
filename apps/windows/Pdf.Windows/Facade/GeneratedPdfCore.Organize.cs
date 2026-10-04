using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed partial class GeneratedPdfCore
{
    public IReadOnlyList<DocumentBlock> DocumentBlocks(IPdfCoreDocument document) =>
        PdfFfiMethods.DocumentBlocks(((GeneratedDocument)document).Handle).Select(block => new DocumentBlock(
            block.Source switch
            {
                FfiBlockSource.Base => DocumentBlockSource.Base,
                FfiBlockSource.Blank => DocumentBlockSource.Blank,
                FfiBlockSource.Imported => DocumentBlockSource.Imported,
                _ => throw new InvalidOperationException("Unknown document block source."),
            }, block.Part, block.Start, block.Count,
            block.Source is FfiBlockSource.Imported imported ? imported.Id : null)).ToArray();
}
