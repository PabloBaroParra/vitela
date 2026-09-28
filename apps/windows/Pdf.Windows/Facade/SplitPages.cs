namespace Pdf.Windows.Facade;

public sealed record SplitPart(uint First, uint Last, string FileName);

public sealed record SplitPagesPlan(IReadOnlyList<SplitPart> Parts, bool SourceIsSigned);
