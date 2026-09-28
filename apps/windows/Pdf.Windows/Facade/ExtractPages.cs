namespace Pdf.Windows.Facade;

/// <summary>What the Extract dialog asked for, before anything is checked.</summary>
public sealed record ExtractPagesRequest(string PageRange);

/// <summary>
/// An extraction whose page range has already been checked — nothing left
/// can be refused by what the reader typed; only the disk can still fail.
/// </summary>
public sealed record ExtractPagesPlan(IReadOnlyList<uint> Pages, bool SourceIsSigned);
