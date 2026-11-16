using Pdf.Windows.Viewer;

/// <summary>
/// Which runs of the open retype editor a style toggle restyles, from the
/// text selected in it — the cases the GTK shell's <c>selection</c> module
/// pins, run against the Windows port.
/// </summary>
internal static class EditorRunSelectionTests
{
    private const string Paragraph = "PRESUPUESTO Nº 3.725/1 Referencia:";
    private static readonly string[] ParagraphRuns = ["PRESUPUESTO Nº", "3.725/1", "Referencia:"];

    public static Task RunAsync()
    {
        ACaretOrNoSelectionIsTheWholeBlock();
        TheWholeTextSelectedIsTheWholeBlock();
        OneRunIsFoundPastItsImpliedSpace();
        WhitespaceAroundTheSelectionDoesNotCount();
        PartOfARunIsRefusedNotRoundedOut();
        ASelectionCoveringOneRunAndPartOfAnotherTakesTheWholeOne();
        APartialSelectionOfEditedTextIsRefused();
        EditedTextWithACaretOrAllOfItSelectedIsTheWholeBlock();
        RepeatedRunTextsAreToldApartByPosition();
        return Task.CompletedTask;
    }

    private static EditorRunSelection Of(string text, string original, string[] runs, (int Start, int End) span) =>
        EditorRunSelection.Of(text, original, runs, span.Start, span.End - span.Start);

    private static (int, int) SpanOf(string text, string needle)
    {
        var start = text.IndexOf(needle, StringComparison.Ordinal);
        if (start < 0) throw new InvalidOperationException($"{needle} is not in {text}");
        return (start, start + needle.Length);
    }

    private static void ACaretOrNoSelectionIsTheWholeBlock()
    {
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, (5, 5)) == EditorRunSelection.Whole, "a caret is the whole block");
    }

    private static void TheWholeTextSelectedIsTheWholeBlock()
    {
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, (0, Paragraph.Length)) == EditorRunSelection.Whole, "all of it");
        Assert(EditorRunSelection.Of(Paragraph, Paragraph, ParagraphRuns, Paragraph.Length, -Paragraph.Length) == EditorRunSelection.Whole,
            "selected backwards");
    }

    private static void OneRunIsFoundPastItsImpliedSpace()
    {
        const string text = "Hello world";
        string[] runs = ["Hello", "world"];
        Assert(Of(text, text, runs, SpanOf(text, "world")).Runs.SequenceEqual([1]), "the second run");
        Assert(Of(text, text, runs, SpanOf(text, "Hello")).Runs.SequenceEqual([0]), "the first run");
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, SpanOf(Paragraph, "PRESUPUESTO Nº 3.725/1")).Runs.SequenceEqual([0, 1]),
            "two runs whole");
    }

    private static void WhitespaceAroundTheSelectionDoesNotCount()
    {
        var (start, end) = SpanOf(Paragraph, "Referencia:");
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, (start - 1, end)).Runs.SequenceEqual([2]), "with the space before it");
        const string text = "Referencia:  next";
        string[] runs = ["Referencia: ", "next"];
        (start, end) = SpanOf(text, "Referencia:");
        Assert(Of(text, text, runs, (start, end + 2)).Runs.SequenceEqual([0]), "with the trailing spaces");
        Assert(Of(text, text, runs, (start, end)).Runs.SequenceEqual([0]), "without the run's own trailing space");
    }

    private static void PartOfARunIsRefusedNotRoundedOut()
    {
        var refused = Of(Paragraph, Paragraph, ParagraphRuns, SpanOf(Paragraph, "Refer"));
        Assert(refused == EditorRunSelection.NoWholeRun && refused.Refusal!.Contains("styled as one piece"), "part of a run");
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, SpanOf(Paragraph, " 3")) is { Kind: EditorRunSelectionKind.NoWholeRun },
            "a space and part of a run");
    }

    private static void ASelectionCoveringOneRunAndPartOfAnotherTakesTheWholeOne()
    {
        Assert(Of(Paragraph, Paragraph, ParagraphRuns, SpanOf(Paragraph, "3.725/1 Refer")).Runs.SequenceEqual([1]), "the whole one only");
    }

    private static void APartialSelectionOfEditedTextIsRefused()
    {
        var edited = Of("AB B", "A B", ["A", "B"], (1, 2));
        Assert(edited == EditorRunSelection.TextEdited && edited.Refusal!.Contains("Commit the edit first"), "typed-in text");
        Assert(Of("AB B", "A B", ["A", "B"], (2, 3)) == EditorRunSelection.TextEdited, "only whitespace of edited text");
    }

    private static void EditedTextWithACaretOrAllOfItSelectedIsTheWholeBlock()
    {
        Assert(Of("AB B", "A B", ["A", "B"], (2, 2)) == EditorRunSelection.Whole, "a caret");
        Assert(Of("AB B", "A B", ["A", "B"], (0, 4)) == EditorRunSelection.Whole, "all of it");
    }

    private static void RepeatedRunTextsAreToldApartByPosition()
    {
        Assert(Of("B B", "B B", ["B", "B"], (2, 3)).Runs.SequenceEqual([1]), "the second of two spaced");
        Assert(Of("B B", "B B", ["B", "B"], (0, 1)).Runs.SequenceEqual([0]), "the first of two spaced");
        Assert(Of("BB", "BB", ["B", "B"], (1, 2)).Runs.SequenceEqual([1]), "the second of two joined");
        Assert(Of("B B B", "B B B", ["B", "B", "B"], (2, 3)).Runs.SequenceEqual([1]), "the middle of three");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
