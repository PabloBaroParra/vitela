using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// FreeText through the facade: the core's contents and layout reach the
/// shell, retyping is one undoable edit, a character the font cannot show is
/// named and changes nothing, and nothing lands on a document that is gone.
/// </summary>
internal static class FreeTextFacadeTests
{
    private static readonly PdfCoreRect Box = new(100, 600, 200, 50);
    private static readonly PdfCoreColor Unused = new(0, 0, 0);

    public static async Task RunAsync()
    {
        await AddCarriesContentsAndLayoutAsync();
        await SetContentsIsOneUndoableEditAsync();
        await UnchangedTextAddsNoHistoryAsync();
        await EncodingGapIsNamedAndChangesNothingAsync();
        await LayoutPassesThroughAndNamesGapsAsync();
        await StaleSessionIsRefusedAsync();
        await ForbiddenDocumentRefusesAsync();
        await AllowsOnlyFreeTextRetypeAsync();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session)> OpenAsync(FakeCore? core = null)
    {
        core ??= new FakeCore();
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("freetext.pdf", [1]))).Value!;
        core.LastDocument!.TrackAnnotationHistory = true;
        return (core, facade, session.SessionId);
    }

    private static Task<OperationResult<AnnotationState>> AddAsync(PdfDocumentFacade facade, string session, string text) =>
        facade.EditAnnotationAsync(session, new PdfCoreEdit.Add(PdfCoreAnnotationKind.FreeText, 0, Box, Unused, Contents: text));

    private static async Task AddCarriesContentsAndLayoutAsync()
    {
        var (_, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        var added = await AddAsync(facade, session, "Hola\nmundo");
        Check(added.IsSuccess, "a valid FreeText is accepted");
        var box = added.Value!.Annotations.Single();
        Check(box.Kind == AnnotationKind.FreeText, "the snapshot says FreeText, not a note");
        Check(box.Contents == "Hola\nmundo", "the typed text comes back as the core stored it");
        Check(box.Rect == new AnnotationRect(100, 600, 200, 50), "the rect the shell chose is the rect stored");
        Check(box.Layout is { Lines.Count: 2 } layout && layout.Lines[0].Text == "Hola" && layout.Lines[1].Text == "mundo",
            "the core's lines reach the shell untouched");
        Check(box.Layout!.FontSizePt == 12, "the core's font size reaches the shell");
        Check(added.Value.CanUndo, "adding is undoable");

        var second = await AddAsync(facade, session, "Otra caja");
        Check(second.Value!.Annotations.Count == 2 && second.Value.Annotations[1].Layout!.Lines.Single().Text == "Otra caja",
            "each box carries its own layout");
    }

    private static async Task SetContentsIsOneUndoableEditAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        var id = (await AddAsync(facade, session, "uno")).Value!.Annotations.Single().Id;

        var retyped = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "dos\ntres"));
        Check(retyped.IsSuccess, "retyping is accepted");
        var box = retyped.Value!.Annotations.Single();
        Check(box.Contents == "dos\ntres", "the new text is stored");
        Check(box.Layout!.Lines.Select(line => line.Text).SequenceEqual(["dos", "tres"]), "the layout follows the new text");
        Check(core.LastDocument!.Annotations.Count == 1, "retyping edits the box, it does not add one");

        await facade.UndoAsync(session);
        var undone = (await facade.AnnotationStateAsync(session)).Value!.Annotations.Single();
        Check(undone.Contents == "uno" && undone.Layout!.Lines.Single().Text == "uno", "one undo restores the text and its layout");
        await facade.RedoAsync(session);
        Check((await facade.AnnotationStateAsync(session)).Value!.Annotations.Single().Contents == "dos\ntres", "redo retypes again");
    }

    private static async Task UnchangedTextAddsNoHistoryAsync()
    {
        var (_, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        var id = (await AddAsync(facade, session, "uno")).Value!.Annotations.Single().Id;
        await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "dos"));
        var same = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "dos"));
        Check(same.IsSuccess, "setting the text it already has is accepted");
        await facade.UndoAsync(session);
        Check((await facade.AnnotationStateAsync(session)).Value!.Annotations.Single().Contents == "uno",
            "the identical retype left no extra undo step behind");
    }

    private static async Task EncodingGapIsNamedAndChangesNothingAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { UnencodableCharacter = "日" });
        using var disposeFacade = facade;
        var refused = await AddAsync(facade, session, "Hola 日本");
        Check(!refused.IsSuccess, "a character the font cannot show refuses the add");
        Check(refused.Error!.Message.Contains('日'), $"the message names the character, got: {refused.Error.Message}");
        Check(core.LastDocument!.Annotations.Count == 0, "nothing was added");
        Check(!(await facade.AnnotationStateAsync(session)).Value!.CanUndo, "no undo entry was created");

        var id = (await AddAsync(facade, session, "Hola")).Value!.Annotations.Single().Id;
        var retype = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "Adiós 日"));
        Check(!retype.IsSuccess && retype.Error!.Message.Contains('日'), "retyping names the character too");
        Check(core.LastDocument.Annotations.Single().Contents == "Hola", "the text is unchanged after the refusal");
    }

    private static async Task LayoutPassesThroughAndNamesGapsAsync()
    {
        var (_, facade, _) = await OpenAsync(new FakeCore { UnencodableCharacter = "日" });
        using var disposeFacade = facade;
        var layout = facade.LayoutFreeText("uno\ndos\ntres", 200, 50);
        Check(layout.IsSuccess && layout.Value!.Lines.Count == 3 && layout.Value.Lines[2].Text == "tres", "a live preview asks the core, not the shell");
        var gap = facade.LayoutFreeText("hola 日", 200, 50);
        Check(!gap.IsSuccess && gap.Error!.Message.Contains('日'), "a preview of unshowable text names the character");
    }

    private static async Task StaleSessionIsRefusedAsync()
    {
        var core = new FakeCore();
        var (_, facade, session) = await OpenAsync(core);
        using var disposeFacade = facade;
        var id = (await AddAsync(facade, session, "uno")).Value!.Annotations.Single().Id;

        var second = (await facade.OpenAsync(new DocumentSource("second.pdf", [2]), discardPendingEdits: true)).Value!;
        Check(!FreeTextInput_SessionMatches(session, second.SessionId), "the dialog's captured session no longer matches");
        var late = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "tarde"));
        Check(!late.IsSuccess && late.Error!.Message == "The document is no longer available.", "a retype for a closed document is refused");
        var lateAdd = await AddAsync(facade, session, "tarde");
        Check(!lateAdd.IsSuccess, "an add for a closed document is refused");
        Check((await facade.AnnotationStateAsync(second.SessionId)).Value!.Annotations.Count == 0, "the new document received nothing");
    }

    private static bool FreeTextInput_SessionMatches(string captured, string? current) => ImageStampInput.SessionMatches(captured, current);

    private static async Task ForbiddenDocumentRefusesAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        var id = (await AddAsync(facade, session, "uno")).Value!.Annotations.Single().Id;
        core.LastDocument!.EditingAllowed = false;
        var add = await AddAsync(facade, session, "dos");
        Check(!add.IsSuccess && add.Error!.Message == "This document or action is not supported.", "a read-only document refuses a new box");
        var retype = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(id, "dos"));
        Check(!retype.IsSuccess && core.LastDocument.Annotations.Single().Contents == "uno", "a read-only document refuses a retype and keeps the text");
    }

    private static async Task AllowsOnlyFreeTextRetypeAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        var shape = (await facade.EditAnnotationAsync(session, new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0, Box, new PdfCoreColor(1, 2, 3)))).Value!.Annotations.Single();
        var refused = await facade.EditAnnotationAsync(session, new PdfCoreEdit.SetContents(shape.Id, "texto"));
        Check(!refused.IsSuccess, "only a FreeText can be retyped");
        Check(core.LastDocument!.Annotations.Single().Contents is null, "the shape did not acquire text");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// The core is faked at the boundary: it lays text out one line per "\n" and
// refuses the configured character, which is all the facade's mapping needs.
sealed partial class FakeCore
{
    public FreeTextLayout LayoutFreeText(string contents, double widthPt, double heightPt)
    {
        if (UnencodableCharacter is { } character && contents.Contains(character, StringComparison.Ordinal))
        {
            throw new PdfCoreException(PdfCoreError.EncodingGap, "EncodingGap", character);
        }
        var lines = contents.Split('\n')
            .Select((text, index) => new FreeTextLine(text, 2, 2 + 0.718 * 12 + index * 13.8))
            .ToArray();
        return new FreeTextLayout(12, lines, lines.Length * 13.8 + 4 > heightPt);
    }

    private bool TryApplyFreeText(FakeDocument fake, PdfCoreEdit edit)
    {
        if (edit is not (PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.FreeText } or PdfCoreEdit.SetContents)) return false;
        if (!fake.EditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "annotation editing is not permitted");
        switch (edit)
        {
            case PdfCoreEdit.Add add:
                var text = add.Contents ?? "";
                if (string.IsNullOrWhiteSpace(text)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "empty free text");
                fake.AddFreeText(add, text, LayoutFreeText(text, add.Rect.Width, add.Rect.Height));
                break;
            case PdfCoreEdit.SetContents set:
                var index = fake.Annotations.FindIndex(annotation => annotation.Id == set.AnnotationId);
                if (index < 0) throw new PdfCoreException(PdfCoreError.AnnotationNotFound, "annotation not found");
                var current = fake.Annotations[index];
                if (current.Kind != PdfCoreAnnotationKind.FreeText) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "not a free text");
                if (string.IsNullOrWhiteSpace(set.Contents)) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "empty free text");
                var layout = LayoutFreeText(set.Contents, current.Rect!.Width, current.Rect.Height);
                if (current.Contents != set.Contents) fake.SetFreeText(index, set.Contents, layout);
                break;
        }
        return true;
    }
}

sealed partial class FakeDocument
{
    public void AddFreeText(PdfCoreEdit.Add add, string contents, FreeTextLayout layout)
    {
        if (TrackAnnotationHistory) RecordAnnotationHistory();
        Annotations.Add(new PdfCoreAnnotation(_nextAnnotationId++, add.PageIndex, PdfCoreAnnotationKind.FreeText, add.Rect, null, [], contents, layout));
        CanUndo = true;
        CanRedo = false;
    }

    public void SetFreeText(int index, string contents, FreeTextLayout layout)
    {
        if (TrackAnnotationHistory) RecordAnnotationHistory();
        Annotations[index] = Annotations[index] with { Contents = contents, Layout = layout };
        CanUndo = true;
        CanRedo = false;
    }
}
