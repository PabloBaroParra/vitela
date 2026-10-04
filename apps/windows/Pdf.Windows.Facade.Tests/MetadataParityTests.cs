using Pdf.Windows.Facade;

internal static class MetadataParityTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        var plus = new MetadataDateOffset(1, 5, 30);
        foreach (var raw in new[] { "2026-02-30", "2026-02-30 08:30", "2026-02-30 08:30:45", "65535-12-31", "0-1-1" })
        {
            var parsed = MetadataDateText.Parse(raw, plus)!;
            Check(parsed.Offset == plus, "Parsing must preserve the supplied offset");
            Check(MetadataDateText.Parse(MetadataDateText.Format(parsed), plus) == parsed, "Components must round-trip without calendar restrictions");
        }
        Check(MetadataDateText.Format(MetadataDateText.Parse("2026-9-1", plus)) == "2026-09-01 00:00:00", "Short dates must normalize to midnight");
        Check(MetadataDateText.Parse(" \t", plus) is null, "Blank input must remove the date");
        foreach (var raw in new[] { "bad", "2026-13-1", "2026-1-0", "2026-1-1 24:00", "2026-1-1 00:60", "2026-1-1 00:00:60", "65536-1-1", "2026-1-1 00", "2026-1-1-2" })
        {
            try { MetadataDateText.Parse(raw, plus); throw new InvalidOperationException($"Accepted invalid date {raw}"); }
            catch (FormatException) { }
        }

        var created = new MetadataDate(2026, 1, 1, 0, 0, 0, plus);
        var modified = created with { Offset = new(-1, 4, 0) };
        var core = new FakeCore { DocumentInfo = new("Original", "Author", null, null, null, null, created, modified) };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("metadata.pdf", [1]))).Value!;
        var read = await facade.DocumentInfoAsync(session.SessionId);
        Check(read.Value!.CreationDate == created && read.Value.ModDate == modified, "Both typed dates must be readable");
        var update = await facade.SetDocumentDateAsync(session.SessionId, DocumentDateProperty.Created, "2026-02-30 08:30");
        Check(update.IsSuccess && update.Value!.CreationDate == new MetadataDate(2026, 2, 30, 8, 30, 0, plus), "Date edits must retain the original offset");
        Check(update.Value!.ModDate == modified && update.Value.Title == "Original", "Unedited properties must survive");
        var before = core.DocumentInfo;
        await facade.SetDocumentDateAsync(session.SessionId, DocumentDateProperty.Created, "2026-02-30 08:30:00");
        Check(ReferenceEquals(core.DocumentInfo, before), "Enter then focus loss must not add a no-op edit");
        var invalid = await facade.SetDocumentDateAsync(session.SessionId, DocumentDateProperty.Created, "2026-99-01");
        Check(!invalid.IsSuccess && invalid.Error!.Message.StartsWith("Invalid date, reverted:") && core.DocumentInfo == before, "Invalid dates must not mutate the document");
        await facade.SetDocumentPropertyAsync(session.SessionId, DocumentProperty.Title, "  New title  ");
        await facade.SetDocumentPropertyAsync(session.SessionId, DocumentProperty.Author, "");
        Check(core.DocumentInfo.Title == "  New title  " && core.DocumentInfo.Author is null, "Individual edits must retain whitespace and remove empty keys");
        Check(Equals(core.DocumentInfo.CreationDate, before.CreationDate) && Equals(core.DocumentInfo.ModDate, modified), "Text editing must not overwrite dates");
        var cleared = await facade.SetDocumentDateAsync(session.SessionId, DocumentDateProperty.Modified, "");
        Check(cleared.IsSuccess && core.DocumentInfo.ModDate is null && core.LastDocument!.CanUndo, "Clearing dates must be undoable");

        var blockedCore = new FakeCore { ContentEditingPermitted = false };
        using var blocked = new PdfDocumentFacade(blockedCore, new RecordingLogger());
        var forbidden = (await blocked.OpenAsync(new DocumentSource("forbidden.pdf", [1]))).Value!;
        Check(!(await blocked.SetDocumentDateAsync(forbidden.SessionId, DocumentDateProperty.Created, "2026-1-1")).IsSuccess, "Dates must enforce content permission");
        Check(!(await blocked.SetDocumentPropertyAsync(forbidden.SessionId, DocumentProperty.Title, "No")).IsSuccess, "Text must enforce content permission");
        Check(!blockedCore.LastDocument!.CanUndo, "Refused metadata must not enter history");
        Check(!(await facade.SetDocumentDateAsync("stale", DocumentDateProperty.Created, "2026-1-1")).IsSuccess, "Stale session must be refused");
    }
}
