using Pdf.Windows.Facade;

/// <summary>
/// GTK #587 on Windows: a run a pending paste put on the page is retyped
/// without a save. The shell sets no rule of its own for it — eligibility
/// asks only about a pending move — and every keystroke reaches the core
/// naming the run the editor opened over; the core checkpoints and rebinds
/// it (pdf-ffi <c>a_pasted_run_is_retyped_without_a_save_keystroke_by_keystroke</c>).
/// </summary>
internal static class PastedRunRetypeTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        var pasted = new PdfCoreContentTextRun((1UL << 40) + 1, 0, new PdfCoreRect(40, 488, 50, 12), "VClip0F1",
            PdfCoreFontKind.Standard14, "Domingo");
        var core = new FakeCore { PageTextRuns = [pasted], CheckpointAvailable = true };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("pasted.pdf", [1]))).Value!;
        var run = (await facade.PageContentAsync(session.SessionId, 0)).Value!.TextRuns.Single();
        Check(ContentSelection.IsPendingId(run.Id), "The fixture run is one a pending edit added");

        Check((await facade.CanRetypeTextRunAsync(session.SessionId, run)).IsSuccess, "A pasted run opens the editor without a save");
        foreach (var text in new[] { "M", "Mi", "Mimi" })
            Check((await facade.ReplaceTextRunAsync(session.SessionId, run, text)).IsSuccess, $"Typing {text} records");
        Check(core.ContentEdits.Count == 3 && core.ContentEdits.All(edit => edit.Item == pasted)
            && core.ContentEdits[^1].After == "Mimi", "Every keystroke names the run the editor opened over");
    }
}
