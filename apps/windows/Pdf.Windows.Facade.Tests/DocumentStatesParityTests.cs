using Pdf.Windows.Facade;

internal static class DocumentStatesParityTests
{
    public static async Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        using var facade = new PdfDocumentFacade(new FakeCore(), new RecordingLogger(), conservativeLifecycle: true);
        var source = new DocumentSource("first.pdf", [1]);
        var session = (await facade.OpenAsync(source)).Value!;
        Check(!(await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "Fresh shell session is clean");
        Check((await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight,
            0, new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)))).IsSuccess, "Record edit");
        var undone = await facade.UndoAsync(session.SessionId);
        Check(undone.IsSuccess && !undone.Value!.CanUndo, "Full Undo fixture");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "Shell must still prompt after full Undo");
        Check((await facade.OpenAsync(source)).Error?.RequiresPendingEditDecision == true, "Open uses conservative guard");
        Check((await facade.CreateBlankAsync()).Error?.RequiresPendingEditDecision == true, "New uses conservative guard");
        Check((await facade.SessionAsync(session.SessionId)).IsSuccess, "Refusals preserve current session");
        Check(!(await facade.SaveToDestinationAsync(session.SessionId, _ => Task.FromException(new IOException("locked")))).IsSuccess,
            "Failed write fixture");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "Failed save cannot clear conservative state");
        Check((await facade.SaveToDestinationAsync(session.SessionId, _ => Task.CompletedTask)).IsSuccess, "Successful write");
        Check(!(await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "Successful destination save clears state");
        Check((await facade.RedoAsync(session.SessionId)).IsSuccess, "Redo after save");
        Check((await facade.HasUnsavedChangesAsync(session.SessionId)).Value, "History after save is dirty again");
        var replacement = await facade.CreateBlankAsync(discardPendingEdits: true);
        Check(replacement.IsSuccess, "Explicit discard replaces session");
        Check(!(await facade.HasUnsavedChangesAsync(session.SessionId)).IsSuccess, "Retired session must be refused");
        Check(!(await facade.HasUnsavedChangesAsync(replacement.Value!.SessionId)).Value, "Replacement resets conservative state");
    }
}
