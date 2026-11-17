namespace Pdf.Windows.Facade;

/// <summary>
/// Text runs that read as one piece of text, as the core groups them
/// (B25 T-207): the member runs' ids line by line, each line left to right,
/// their joined box and text. A paragraph (PR #568) has more than one line,
/// its text joined with <c>'\n'</c>, and the mean baseline-to-baseline
/// distance in points as its leading.
/// </summary>
internal sealed record PdfCoreTextBlock(IReadOnlyList<ulong> RunIds, PdfCoreRect Bbox, string Text,
    int LineCount = 1, double? Leading = null);

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
    internal ContentTextBlock(IReadOnlyList<ContentTextRun> runs, AnnotationRect bounds, string text,
        int lineCount = 1, double? leading = null)
        => (Runs, Bounds, Text, LineCount, Leading) = (runs, bounds, text, Math.Max(1, lineCount), leading);

    /// <summary>The member runs, line by line from the top, each line left to right.</summary>
    public IReadOnlyList<ContentTextRun> Runs { get; }

    /// <summary>The union of the runs' boxes, PDF space, bottom-left origin.</summary>
    public AnnotationRect Bounds { get; }

    /// <summary>The runs' text joined left to right, a paragraph's lines joined with <c>'\n'</c>.</summary>
    public string Text { get; }

    /// <summary>How many lines the block has: one unless it is a paragraph.</summary>
    public int LineCount { get; }

    /// <summary>A paragraph's baseline-to-baseline distance in points; <c>null</c> for one line.</summary>
    public double? Leading { get; }

    /// <summary>
    /// Whether this is a paragraph — lines stacked a steady leading apart,
    /// edited in a multi-line box and retyped as a whole, the core wrapping
    /// the new text to its width.
    /// </summary>
    public bool IsParagraph => LineCount > 1;

    /// <summary>
    /// <paramref name="text"/> with every line break as the line feed the core
    /// splits a paragraph on — a WinUI text box stores one as a carriage
    /// return, one character for one.
    /// </summary>
    public static string WithLineFeeds(string text) =>
        text.Replace("\r\n", "\n", StringComparison.Ordinal).Replace('\r', '\n');

    public uint PageIndex => Runs[0].PageIndex;

    /// <summary>The run whose font, id and editor a retype of the whole block uses.</summary>
    public ContentTextRun First => Runs[0];

    /// <summary>Whether this is more than one run — the case the block commands exist for.</summary>
    public bool IsMultiRun => Runs.Count > 1;
}
