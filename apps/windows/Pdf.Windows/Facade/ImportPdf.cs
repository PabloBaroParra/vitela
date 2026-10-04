namespace Pdf.Windows.Facade;

/// <summary>
/// What one "Add PDFs" file did: the session with its new page layout, how
/// many pages arrived, and what they could not bring across exactly.
/// </summary>
/// <param name="SourceId">
/// The id the pages' block carries as <see cref="DocumentBlock.ImportedSourceId"/>
/// — how the shell names a block after the file it came from.
/// </param>
public sealed record ImportedPdf(DocumentSession Session, uint PageCount, IReadOnlyList<string> Warnings, ulong SourceId);

/// <summary>The core's <c>FfiImportReport</c>, on this side of the boundary.</summary>
internal sealed record PdfCoreImportReport(uint PageCount, IReadOnlyList<string> Warnings, ulong SourceId);
