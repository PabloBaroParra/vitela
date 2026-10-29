namespace Pdf.Windows.Facade;

/// <summary>
/// Text runs that read as one piece of text, as the core groups them
/// (B25 T-207): the member runs' ids left to right, their joined box and text.
/// </summary>
internal sealed record PdfCoreTextBlock(IReadOnlyList<ulong> RunIds, PdfCoreRect Bbox, string Text);

/// <summary>
/// A whole-block edit. Each one is a single undo step in the core, which also
/// decides the order its pieces are written in.
/// </summary>
internal abstract record PdfCoreTextBlockEdit(IReadOnlyList<PdfCoreContentTextRun> Runs)
{
    public sealed record Move(IReadOnlyList<PdfCoreContentTextRun> Runs, double Dx, double Dy) : PdfCoreTextBlockEdit(Runs);
    public sealed record Remove(IReadOnlyList<PdfCoreContentTextRun> Runs) : PdfCoreTextBlockEdit(Runs);
    public sealed record Retype(IReadOnlyList<PdfCoreContentTextRun> Runs, string Text, bool WithInsertedFont) : PdfCoreTextBlockEdit(Runs);
}

/// <summary>
/// Text runs that read as one piece of text — the unit content-edit mode
/// selects, drags, deletes and retypes.
/// </summary>
/// <remarks>
/// A run that joins nothing is a block of one, so every run on the page is in
/// exactly one block and a shell can hit-test blocks alone.
/// </remarks>
public sealed class ContentTextBlock
{
    internal ContentTextBlock(IReadOnlyList<ContentTextRun> runs, AnnotationRect bounds, string text)
        => (Runs, Bounds, Text) = (runs, bounds, text);

    /// <summary>The member runs, left to right.</summary>
    public IReadOnlyList<ContentTextRun> Runs { get; }

    /// <summary>The union of the runs' boxes, PDF space, bottom-left origin.</summary>
    public AnnotationRect Bounds { get; }

    /// <summary>The runs' text joined left to right.</summary>
    public string Text { get; }

    public uint PageIndex => Runs[0].PageIndex;

    /// <summary>The run whose font, id and editor a retype of the whole block uses.</summary>
    public ContentTextRun First => Runs[0];

    /// <summary>Whether this is more than one run — the case the block commands exist for.</summary>
    public bool IsMultiRun => Runs.Count > 1;
}
