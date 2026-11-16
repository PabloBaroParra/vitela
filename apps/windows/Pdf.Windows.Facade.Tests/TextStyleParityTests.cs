using Pdf.Windows.Facade;

/// <summary>
/// Bold / italic / underline toggles on existing text through the facade:
/// the buttons follow the first run, a press evens a mixed target out as one
/// core call, and a style that is the run's own font with no plain sibling
/// stays on.
/// </summary>
internal static class TextStyleParityTests
{
    private static readonly PdfCoreContentTextRun Hello = new(0, 0, new PdfCoreRect(72, 700, 30, 12), "F1", PdfCoreFontKind.Standard14, "Hello");
    private static readonly PdfCoreContentTextRun World = new(1, 0, new PdfCoreRect(102, 700, 32, 12), "F2", PdfCoreFontKind.Standard14, "world");
    private static readonly PdfCoreContentTextRun Again = new(2, 0, new PdfCoreRect(134, 700, 32, 12), "F1", PdfCoreFontKind.Standard14, "again");

    private static readonly TextRunLook Plain = default;
    private static readonly TextRunLook BoldLook = new(true, false, false, false);

    public static async Task RunAsync()
    {
        TheButtonsFollowTheFirstRun();
        await ReadsTheButtonsFromTheCoreAsync();
        await AnUnreadableRunDisablesTheButtonsAsync();
        await APressEvensAMixedTargetOutInOneCallAsync();
        await AFixedBoldIsRefusedBeforeTheCoreAsync();
        await TheCoresRefusalReachesTheReaderAsync();
        await AStaleSessionIsRefusedAsync();
    }

    private static void TheButtonsFollowTheFirstRun()
    {
        var fixedBold = new TextRunLookState(BoldLook, CanRemoveBold: false, CanRemoveItalic: true);
        var plain = new TextRunLookState(Plain, true, true);

        var buttons = TextStylePolicy.Buttons([plain, fixedBold])!;
        Assert(buttons.Look == Plain && buttons.BoldRemovable && !buttons.IsFixed(TextStyleToggle.Bold),
            "a plain first run can always be made bold");

        buttons = TextStylePolicy.Buttons([fixedBold, plain])!;
        Assert(buttons.Look.Bold && buttons.IsFixed(TextStyleToggle.Bold) && !buttons.IsFixed(TextStyleToggle.Italic),
            "a native bold with no regular sibling shows pressed and fixed");

        Assert(TextStylePolicy.Buttons([]) is null, "no target shows no buttons");
        var (restyles, refusal) = TextStylePolicy.Restyles([], TextStyleToggle.Italic);
        Assert(restyles.Count == 0 && refusal is null, "no target records nothing");
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session, IReadOnlyList<ContentTextRun> Runs)> OpenAsync(
        params TextRunLookState?[] looks)
    {
        var core = new FakeCore { PageTextRuns = [Hello, World, Again] };
        for (var index = 0; index < looks.Length; index++) core.RunLooks[(ulong)index] = looks[index];
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("styled.pdf", [1]))).Value!;
        var runs = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns;
        return (core, facade, session.SessionId, runs);
    }

    private static async Task ReadsTheButtonsFromTheCoreAsync()
    {
        var (_, facade, session, runs) = await OpenAsync(
            new TextRunLookState(new TextRunLook(false, true, true, false), true, true),
            new TextRunLookState(BoldLook, false, true));
        var read = await facade.TextStyleButtonsAsync(session, runs);
        Assert(read.IsSuccess && read.Value is { } buttons && buttons.Look.Italic && buttons.Look.Underline && !buttons.Look.Bold,
            "the first run's look, queued restyles included, is what the buttons show");
    }

    private static async Task AnUnreadableRunDisablesTheButtonsAsync()
    {
        var (_, facade, session, runs) = await OpenAsync(new TextRunLookState(Plain, true, true), null);
        var read = await facade.TextStyleButtonsAsync(session, runs);
        Assert(read.IsSuccess && read.Value is null, "a run whose look cannot be read leaves the toggles off");
    }

    private static async Task APressEvensAMixedTargetOutInOneCallAsync()
    {
        var (core, facade, session, runs) = await OpenAsync(
            new TextRunLookState(Plain, true, true),
            new TextRunLookState(BoldLook, true, true),
            new TextRunLookState(Plain, true, true));
        var pressed = await facade.ToggleTextStyleAsync(session, runs, TextStyleToggle.Bold);
        Assert(pressed.IsSuccess && pressed.Value!.CanUndo && core.RefreshPreviewCalls == 1, "a press refreshes and enters history");
        var restyles = core.Restyles.Single();
        Assert(restyles.Select(restyle => restyle.Item).SequenceEqual([Hello, Again]), "the run already bold is left out");
        Assert(restyles.All(restyle => restyle.From == Plain && restyle.To == BoldLook), "each run goes from its own look to bold");
    }

    private static async Task AFixedBoldIsRefusedBeforeTheCoreAsync()
    {
        var (core, facade, session, runs) = await OpenAsync(
            new TextRunLookState(BoldLook, CanRemoveBold: false, CanRemoveItalic: true),
            new TextRunLookState(BoldLook, true, true));
        var refused = await facade.ToggleTextStyleAsync(session, runs, TextStyleToggle.Bold);
        Assert(!refused.IsSuccess && refused.Error!.Message.Contains("cannot be made regular"), "the reason reaches the reader");
        Assert(core.Restyles.Count == 0 && core.RefreshPreviewCalls == 0, "nothing reaches the core");

        var italic = await facade.ToggleTextStyleAsync(session, runs, TextStyleToggle.Italic);
        Assert(italic.IsSuccess && core.Restyles.Single().Count == 3, "italic still goes on over a fixed bold");
    }

    private static async Task TheCoresRefusalReachesTheReaderAsync()
    {
        var (core, facade, session, runs) = await OpenAsync(new TextRunLookState(Plain, true, true));
        core.RestyleRefusal = "This text has an unsaved edit — save the document before editing it again.";
        var refused = await facade.ToggleTextStyleAsync(session, [runs[0]], TextStyleToggle.Underline);
        Assert(!refused.IsSuccess && refused.Error!.Message.Contains("save the document"), "the core's sentence is shown as is");
        Assert(core.RefreshPreviewCalls == 0, "a refused restyle rebuilds no preview");
    }

    private static async Task AStaleSessionIsRefusedAsync()
    {
        var (core, facade, _, runs) = await OpenAsync(new TextRunLookState(Plain, true, true));
        var refused = await facade.ToggleTextStyleAsync("another-session", runs, TextStyleToggle.Bold);
        Assert(!refused.IsSuccess && core.Restyles.Count == 0, "a closed document is never restyled");
    }

    private static void Assert(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// Looks come from RunLooks (plain for a run a test does not name, null for
// one it names as unreadable); restyles are recorded in order.
sealed partial class FakeCore
{
    public Dictionary<ulong, TextRunLookState?> RunLooks { get; } = [];
    public List<IReadOnlyList<PdfCoreTextRunRestyle>> Restyles { get; } = [];
    public string? RestyleRefusal { get; set; }

    public TextRunLookState? ReadTextRunLook(IPdfCoreDocument document, PdfCoreContentTextRun run) =>
        RunLooks.TryGetValue(run.Id, out var look) ? look : new TextRunLookState(default, true, true);

    public void RestyleTextRuns(IPdfCoreDocument document, IReadOnlyList<PdfCoreTextRunRestyle> restyles)
    {
        if (RestyleRefusal is { } refusal) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "UnsupportedOperation", refusal);
        Restyles.Add(restyles);
        // As for a block edit: recording a content edit is what gives the
        // facade the history a real restyle leaves.
        ((FakeDocument)document).Apply(new PdfCoreEdit.ReplaceTextRun(restyles[0].Item, restyles[0].Item.Text));
    }
}
