using Pdf.Windows.Facade;

internal static class AnnotationStyleParityTests
{
    public static async Task RunAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        var core = new FakeCore();
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("color.pdf", [1]))).Value!;
        core.LastDocument!.TrackAnnotationHistory = true;
        var added = await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(1, 2, 3)));
        var before = added.Value!.Annotations.Single();
        var changed = await facade.RestyleAnnotationAsync(session.SessionId, before with { Points = before.Points.ToArray() }, new(10, 20, 30));
        Check(changed.IsSuccess && changed.Value!.Annotations.Single().Color == new AnnotationColor(10, 20, 30), "Captured points must compare by value");
        var stale = await facade.RestyleAnnotationAsync(session.SessionId, before, new(40, 50, 60));
        Check(!stale.IsSuccess && stale.Error!.Message == "Color selection is no longer current.", "Stale color must not overwrite");
        var current = changed.Value!.Annotations.Single();
        await facade.UndoAsync(session.SessionId);
        Check((await facade.AnnotationStateAsync(session.SessionId)).Value!.Annotations.Single().Color == before.Color, "Color edit must undo in one step");
        await facade.RedoAsync(session.SessionId);
        var same = await facade.RestyleAnnotationAsync(session.SessionId, current, current.Color!);
        Check(same.IsSuccess, "Unchanged color must succeed without mutation");
        await facade.UndoAsync(session.SessionId);
        Check((await facade.AnnotationStateAsync(session.SessionId)).Value!.Annotations.Single().Color == before.Color, "No-op must not add history");
        core.LastDocument!.EditingAllowed = false;
        Check(!(await facade.RestyleAnnotationAsync(session.SessionId, before, new(40, 50, 60))).IsSuccess, "Permission refusal");
        core.LastDocument.EditingAllowed = true;
        var moved = core.LastDocument.Annotations.Single();
        core.LastDocument.Annotations[0] = moved with { Rect = moved.Rect! with { X = moved.Rect.X + 1 } };
        Check(!(await facade.RestyleAnnotationAsync(session.SessionId, before, new(40, 50, 60))).IsSuccess, "Moved target must be stale");
        await facade.EditAnnotationAsync(session.SessionId, new PdfCoreEdit.Remove(before.Id));
        Check(!(await facade.RestyleAnnotationAsync(session.SessionId, before, new(40, 50, 60))).IsSuccess, "Deleted target must be stale");
        Check(!(await facade.RestyleAnnotationAsync("stale", before, new(40, 50, 60))).IsSuccess, "Retired session must be refused");
    }
}

// Opt-in annotation history: legacy fake users retain their flag-only model.
// The color fixture needs actual values restored to prove one-step/no-op behavior.
sealed partial class FakeDocument
{
    public bool TrackAnnotationHistory { get; set; }
    private readonly Stack<PdfCoreAnnotation[]> _annotationUndo = new();
    private readonly Stack<PdfCoreAnnotation[]> _annotationRedo = new();

    private void RecordAnnotationHistory()
    {
        _annotationUndo.Push(Annotations.ToArray());
        _annotationRedo.Clear();
    }

    private bool RestoreAnnotationHistory(bool undo)
    {
        var from = undo ? _annotationUndo : _annotationRedo;
        var to = undo ? _annotationRedo : _annotationUndo;
        if (from.Count == 0) return false;
        to.Push(Annotations.ToArray());
        Annotations.Clear();
        Annotations.AddRange(from.Pop());
        CanUndo = _annotationUndo.Count > 0;
        CanRedo = _annotationRedo.Count > 0;
        return true;
    }
}
