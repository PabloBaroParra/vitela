using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>What a key pressed in the paragraph editor does.</summary>
public enum ParagraphKeyAction
{
    /// <summary>Ctrl+Enter: write the paragraph and close the box.</summary>
    Commit,

    /// <summary>Escape: put the paragraph back and close the box.</summary>
    Cancel,

    /// <summary>Anything else, plain Enter among it: the box's own — a line break.</summary>
    Type,
}

/// <summary>
/// The rules of the multi-line editor content-edit mode opens over a
/// paragraph block — free of WinUI so they can be tested without a window.
/// The Windows port of the GTK shell's <c>content_edit::paragraph_editor</c>.
/// </summary>
/// <remarks>
/// A paragraph's line breaks are part of what the reader retypes, and the
/// core wraps each typed line to the paragraph's width
/// (<c>retype_text_block</c>). So <b>Enter</b> is a line break,
/// <b>Ctrl+Enter</b> (or a click elsewhere) commits, and <b>Escape</b>
/// cancels, exactly as on Linux.
/// </remarks>
public static class ParagraphEditing
{
    /// <summary>What a key does: <paramref name="enter"/> and <paramref name="escape"/> name the key, <paramref name="control"/> whether Ctrl is held.</summary>
    public static ParagraphKeyAction KeyAction(bool enter, bool escape, bool control) =>
        escape ? ParagraphKeyAction.Cancel
        : enter && control ? ParagraphKeyAction.Commit
        : ParagraphKeyAction.Type;

    /// <summary>
    /// A text box's text with every line break as <c>'\n'</c>, the break the
    /// core joins a paragraph's lines with. A WinUI <c>TextBox</c> stores a
    /// typed line break as <c>'\r'</c>; one character for one, so a caret
    /// index means the same in both.
    /// </summary>
    public static string FromTextBox(string text) => ContentTextBlock.WithLineFeeds(text);

    /// <summary>
    /// Where the caret goes in <paramref name="text"/>, a block's text, for a
    /// click <paramref name="offset"/> characters into run
    /// <paramref name="clicked"/> of <paramref name="runs"/> (their texts, in
    /// the block's order): each run is found where the one before it ended, so
    /// a second run reading like the first is not mistaken for it. The end of
    /// the text when a run is not where the text before it says.
    /// </summary>
    public static int CaretIndex(string text, IReadOnlyList<string> runs, int clicked, int offset)
    {
        var from = 0;
        for (var index = 0; index < runs.Count && index <= clicked; index++)
        {
            var at = text.IndexOf(runs[index], from, StringComparison.Ordinal);
            if (at < 0) return text.Length;
            if (index == clicked) return Math.Clamp(at + offset, 0, text.Length);
            from = at + runs[index].Length;
        }
        return text.Length;
    }

    /// <summary>
    /// The block in <paramref name="blocks"/>, a page read again after a
    /// write, that is the paragraph <paramref name="opened"/> was: the one
    /// holding any of its runs, or — when a centred or right-aligned
    /// paragraph was painted afresh and every run has a new id — the one
    /// under the middle of its first line and reads <paramref name="written"/>,
    /// whitespace aside (the core's wrap moves line breaks). <c>null</c> when
    /// neither is there.
    /// </summary>
    /// <remarks>
    /// A paragraph retyped again has to name the runs the page holds now: a
    /// retype built from the runs it opened over is refused once an earlier
    /// write grew a line. A neighbour under that point is never taken for it:
    /// left unfound, the next write is refused by the core, visibly.
    /// </remarks>
    public static ContentTextBlock? Refind(IReadOnlyList<ContentTextBlock> blocks, ContentTextBlock opened, string written)
    {
        // Whichever way it is found, it must read what was written: ids are
        // renumbered after a checkpoint, and a neighbour can sit under the point.
        static string Letters(string text) => string.Concat(text.Where(c => !char.IsWhiteSpace(c)));
        var letters = Letters(written);
        var reads = blocks.Where(block => Letters(block.Text) == letters).ToArray();
        var ids = opened.Runs.Select(run => run.Id).ToHashSet();
        if (reads.FirstOrDefault(block => block.Runs.Any(run => ids.Contains(run.Id))) is { } kept) return kept;
        var first = opened.First.Bounds;
        var (x, y) = (first.X + first.Width / 2, first.Y + first.Height / 2);
        return reads.FirstOrDefault(block => block.Bounds.X <= x && x <= block.Bounds.X + block.Bounds.Width
            && block.Bounds.Y <= y && y <= block.Bounds.Y + block.Bounds.Height);
    }
}
