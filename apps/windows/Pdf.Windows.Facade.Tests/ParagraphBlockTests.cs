using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// Paragraph blocks on Windows, as on Linux (PR #568): the core's stacked
/// lines arrive as one block that knows it is a paragraph, its retype reaches
/// the core with <c>'\n'</c> line breaks whatever the text box wrote, and the
/// multi-line editor's keys and re-read follow the GTK shell's
/// <c>content_edit::paragraph_editor</c>.
/// </summary>
internal static class ParagraphBlockTests
{
    private static readonly PdfCoreContentTextRun Composicion = new(0, 0, new PdfCoreRect(60, 600, 300, 10), "F1", PdfCoreFontKind.EmbeddedSimple, "COMPOSICIÓN: Aluminio");
    private static readonly PdfCoreContentTextRun Color = new(1, 0, new PdfCoreRect(60, 586, 150, 10), "F1", PdfCoreFontKind.EmbeddedSimple, "COLOR: Blanco");
    private static readonly PdfCoreContentTextRun Total = new(2, 0, new PdfCoreRect(400, 500, 27, 10), "F1", PdfCoreFontKind.EmbeddedSimple, "Total");

    public static async Task RunAsync()
    {
        await AParagraphArrivesAsOneBlockThatKnowsItsLinesAsync();
        await AParagraphRetypeSendsLineFeedsWhateverTheBoxWroteAsync();
        TextBoxLineBreaksBecomeLineFeeds();
        PlainEnterIsALineBreakAndCtrlEnterCommits();
        AReReadParagraphIsFoundByARunItKeptOrWhereItStood();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session)> OpenAsync()
    {
        var core = new FakeCore
        {
            PageTextRuns = [Composicion, Color, Total],
            PageTextBlocks =
            [
                new([Composicion.Id, Color.Id], new PdfCoreRect(60, 586, 300, 24), "COMPOSICIÓN: Aluminio\nCOLOR: Blanco", 2, 14),
                new([Total.Id], Total.Bbox, "Total"),
            ],
        };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("quote.pdf", [1]))).Value!;
        return (core, facade, session.SessionId);
    }

    private static async Task AParagraphArrivesAsOneBlockThatKnowsItsLinesAsync()
    {
        var (_, facade, session) = await OpenAsync();
        var content = (await facade.PageContentAsync(session, 0)).Value!;
        var paragraph = content.TextBlocks[0];
        Assert(paragraph.IsParagraph && paragraph.LineCount == 2 && paragraph.Leading == 14,
            "a stacked block arrives as a paragraph with its line count and leading");
        Assert(paragraph.Text == "COMPOSICIÓN: Aluminio\nCOLOR: Blanco", "a paragraph's text keeps its line breaks");
        Assert(ReferenceEquals(content.BlockOf(content.TextRuns[1]), paragraph), "a run on the second line finds the paragraph");
        var single = content.TextBlocks[1];
        Assert(!single.IsParagraph && single.LineCount == 1 && single.Leading is null, "a one-line block is not a paragraph");
    }

    private static async Task AParagraphRetypeSendsLineFeedsWhateverTheBoxWroteAsync()
    {
        var (core, facade, session) = await OpenAsync();
        var paragraph = (await facade.PageContentAsync(session, 0)).Value!.TextBlocks[0];
        var retyped = await facade.RetypeTextBlockAsync(session, paragraph, "COMPOSICIÓN: Acero\rCOLOR: Negro\r\nAPERTURA: Corredera");
        Assert(retyped.IsSuccess, "a paragraph retype is accepted");
        Assert(core.TextBlockEdits.Single() is PdfCoreTextBlockEdit.Retype
            {
                Text: "COMPOSICIÓN: Acero\nCOLOR: Negro\nAPERTURA: Corredera",
                WithInsertedFont: false,
            } retype && retype.Runs.SequenceEqual([Composicion, Color]),
            "the core gets every run of the paragraph and '\\n' line breaks");
    }

    private static void TextBoxLineBreaksBecomeLineFeeds()
    {
        Assert(ParagraphEditing.FromTextBox("A\rB\r\nC\nD") == "A\nB\nC\nD", "every kind of line break reads as '\\n'");
        Assert(ParagraphEditing.FromTextBox("one line") == "one line", "a line without breaks is unchanged");
    }

    private static void PlainEnterIsALineBreakAndCtrlEnterCommits()
    {
        Assert(ParagraphEditing.KeyAction(enter: true, escape: false, control: false) == ParagraphKeyAction.Type,
            "plain Enter types a line break");
        Assert(ParagraphEditing.KeyAction(enter: true, escape: false, control: true) == ParagraphKeyAction.Commit,
            "Ctrl+Enter commits");
        Assert(ParagraphEditing.KeyAction(enter: false, escape: true, control: false) == ParagraphKeyAction.Cancel,
            "Escape cancels");
        Assert(ParagraphEditing.KeyAction(enter: false, escape: false, control: true) == ParagraphKeyAction.Type,
            "any other key is the box's own");
    }

    private static void AReReadParagraphIsFoundByARunItKeptOrWhereItStood()
    {
        ContentTextRun Run(PdfCoreContentTextRun source) => new(source, "Helvetica");
        ContentTextBlock Block(params ContentTextRun[] runs) => new(runs,
            new AnnotationRect(runs.Min(run => run.Bounds.X), runs.Min(run => run.Bounds.Y),
                runs.Max(run => run.Bounds.X + run.Bounds.Width) - runs.Min(run => run.Bounds.X),
                runs.Max(run => run.Bounds.Y + run.Bounds.Height) - runs.Min(run => run.Bounds.Y)),
            string.Join("\n", runs.Select(run => run.Text)), runs.Length, runs.Length > 1 ? 14 : null);

        var opened = Block(Run(Composicion), Run(Color));
        var grown = Block(Run(Composicion), Run(Color), Run(Composicion with { Id = (1UL << 40) + 1, Bbox = new PdfCoreRect(60, 572, 90, 10), Text = "UNA" }));
        var total = Block(Run(Total));
        Assert(ReferenceEquals(ParagraphEditing.Refind([total, grown], opened, "anything"), grown), "the paragraph that kept a run is found");

        // A centred paragraph is painted afresh: every run has a new id. Found
        // where its first line stood only when it reads what was written there,
        // the core's wrap aside.
        var repainted = Block(Run(Composicion with { Id = (1UL << 40) + 7 }), Run(Color with { Id = (1UL << 40) + 8 }));
        Assert(ReferenceEquals(ParagraphEditing.Refind([total, repainted], opened, "COMPOSICIÓN: Aluminio COLOR:\nBlanco"), repainted),
            "a repainted paragraph is found where its first line stood");
        Assert(ParagraphEditing.Refind([total, repainted], opened, "Something else") is null,
            "a neighbouring block that does not read the written text is not retargeted");
        Assert(ParagraphEditing.Refind([total], opened, "Total") is null, "a paragraph that is gone is not guessed at");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
