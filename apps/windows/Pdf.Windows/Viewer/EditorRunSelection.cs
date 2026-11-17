namespace Pdf.Windows.Viewer;

/// <summary>What a style toggle pressed with text selected in the open retype editor acts on.</summary>
public enum EditorRunSelectionKind
{
    /// <summary>Every run: a caret only, or the whole text selected.</summary>
    Whole,

    /// <summary>Only the runs in <see cref="EditorRunSelection.Runs"/>.</summary>
    Runs,

    /// <summary>A selection that covers no run whole — refused, never rounded out.</summary>
    NoWholeRun,

    /// <summary>Part of a text the reader has typed in: its runs can no longer be told apart.</summary>
    TextEdited,
}

/// <summary>
/// Which runs of the open editor's text a style toggle restyles, from the text
/// the reader selected in it — free of WinUI so it can be tested without a
/// window. The Windows port of the GTK shell's
/// <c>content_edit::style_toggles::selection</c>.
/// </summary>
/// <remarks>
/// The core restyles whole runs only, so a selection is mapped onto the runs
/// it covers. The editor shows the block's text as the core joined it — the
/// runs left to right, joined by nothing or by an implied space — and each run
/// is found in that text in turn, where the previous one ended or after the
/// whitespace that follows it. That only holds for the text the editor opened
/// with: once the reader has typed, a selection of part of it is refused, and
/// only a caret or the whole text still means every run. Whitespace at either
/// end of the selection or of a run does not count.
/// </remarks>
public sealed record EditorRunSelection(EditorRunSelectionKind Kind, IReadOnlyList<int> Runs)
{
    public static readonly EditorRunSelection Whole = new(EditorRunSelectionKind.Whole, []);
    public static readonly EditorRunSelection NoWholeRun = new(EditorRunSelectionKind.NoWholeRun, []);
    public static readonly EditorRunSelection TextEdited = new(EditorRunSelectionKind.TextEdited, []);

    /// <summary>The reader-facing reason a press with this selection records nothing, or <c>null</c>.</summary>
    public string? Refusal => Kind switch
    {
        EditorRunSelectionKind.NoWholeRun => "The selection splits text that is styled as one piece. Select all of that piece to restyle it.",
        EditorRunSelectionKind.TextEdited => "The text was changed. Commit the edit first, then select the part to restyle.",
        _ => null,
    };

    /// <summary>
    /// The runs of <paramref name="runs"/> (their texts, in the block's order)
    /// that the selection <paramref name="start"/>..+<paramref name="length"/>
    /// covers in <paramref name="text"/>, the editor's text;
    /// <paramref name="original"/> is the text it opened with.
    /// </summary>
    public static EditorRunSelection Of(string text, string original, IReadOnlyList<string> runs, int start, int length)
    {
        var from = Math.Clamp(Math.Min(start, start + length), 0, text.Length);
        var to = Math.Clamp(Math.Max(start, start + length), 0, text.Length);
        if (from >= to) return Whole;
        var selected = Trim(text, from, to);
        if (selected is { } inner && Trim(text, 0, text.Length) is { } all && Contains(inner, all)) return Whole;
        // Before the whitespace test: in edited text even a selected space
        // cannot be placed against the runs.
        if (text != original) return TextEdited;
        if (selected is not { } span) return NoWholeRun;
        var covered = Locate(text, runs)
            .Select((found, index) => (found, index))
            .Where(pair => pair.found is { } run && run.Start < run.End
                && Contains(span, Trim(text, run.Start, run.End) ?? run))
            .Select(pair => pair.index)
            .ToArray();
        return covered.Length == 0 ? NoWholeRun
            : covered.Length == runs.Count ? Whole
            : new EditorRunSelection(EditorRunSelectionKind.Runs, covered);
    }

    /// <summary>Where each run sits in <paramref name="text"/>; <c>null</c> from the first one that is not where the text before it says.</summary>
    private static (int Start, int End)?[] Locate(string text, IReadOnlyList<string> runs)
    {
        var found = new (int Start, int End)?[runs.Count];
        var cursor = 0;
        for (var index = 0; index < runs.Count; index++)
        {
            var run = runs[index];
            var afterGap = cursor;
            while (afterGap < text.Length && char.IsWhiteSpace(text[afterGap])) afterGap++;
            int? position = At(cursor) ? cursor : At(afterGap) ? afterGap : null;
            if (position is not { } at) break;
            cursor = at + run.Length;
            found[index] = (at, cursor);

            bool At(int offset) => string.CompareOrdinal(text, offset, run, 0, run.Length) == 0
                && offset + run.Length <= text.Length;
        }
        return found;
    }

    private static (int Start, int End)? Trim(string text, int start, int end)
    {
        while (start < end && char.IsWhiteSpace(text[start])) start++;
        while (end > start && char.IsWhiteSpace(text[end - 1])) end--;
        return start < end ? (start, end) : null;
    }

    private static bool Contains((int Start, int End) outer, (int Start, int End) inner) =>
        outer.Start <= inner.Start && inner.End <= outer.End;
}
