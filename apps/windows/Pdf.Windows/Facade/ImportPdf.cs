namespace Pdf.Windows.Facade;

/// <summary>
/// One picked PDF, opened and checked but not yet added. Dispose it to
/// abandon the import; once imported it is spent.
/// </summary>
public interface IImportSource : IDisposable
{
    uint PageCount { get; }

    /// <summary>What its pages will not bring across exactly — known before anything is added.</summary>
    IReadOnlyList<string> Warnings { get; }
}

/// <summary>
/// What one "Add PDFs" pick did: the session with its new page layout, how
/// many pages arrived, and each source's <see cref="DocumentBlock.ImportedSourceId"/>
/// in pick order — how the shell names a block after its file.
/// </summary>
public sealed record ImportedPdfs(DocumentSession Session, uint PageCount, IReadOnlyList<ulong> SourceIds);

/// <summary>The core's <c>FfiBatchImportReport</c>, on this side of the boundary.</summary>
internal sealed record PdfCoreBatchImportReport(uint PageCount, IReadOnlyList<ulong> SourceIds);
