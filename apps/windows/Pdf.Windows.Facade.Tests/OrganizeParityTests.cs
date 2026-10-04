using Pdf.Windows.Facade;

internal static class OrganizeParityTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        Check(Pdf.Windows.Viewer.DragEdgeScroll.Step(300, 600) == 0 && Pdf.Windows.Viewer.DragEdgeScroll.Step(45, 90) == 0,
            "Normal and short viewports retain a neutral middle");
        Check(Pdf.Windows.Viewer.DragEdgeScroll.Step(0, 600) == -16 && Pdf.Windows.Viewer.DragEdgeScroll.Step(600, 600) == 16
            && Pdf.Windows.Viewer.DragEdgeScroll.Step(0, 0) == 0, "Edge speed is bounded and unlaid-out views do not scroll");
        Check(Pdf.Windows.Viewer.DragEdgeScroll.Step(2, 600) < Pdf.Windows.Viewer.DragEdgeScroll.Step(54, 600)
            && Pdf.Windows.Viewer.DragEdgeScroll.Step(598, 600) > Pdf.Windows.Viewer.DragEdgeScroll.Step(546, 600), "Edge speed must ramp with depth");
        var core = new FakeCore { PageCount = 6, Blocks = [new(DocumentBlockSource.Base, null, 0, 2),
            new(DocumentBlockSource.Blank, null, 2, 3), new(DocumentBlockSource.Imported, null, 5, 1)] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new("blocks.pdf", [1]))).Value!;
        async Task<DocumentBlocksSnapshot> Read() => (await facade.DocumentBlocksAsync(session.SessionId)).Value!;
        var snapshot = await Read();
        Check(snapshot.Blocks.SequenceEqual(core.Blocks), "Blocks must come from core, not shell grouping");
        Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(snapshot with { SessionId = "other" }, 0, DocumentBlockAction.Delete))).IsSuccess
            && core.PageEdits.Count == 0, "Same-layout snapshot from another document must be refused");
        foreach (var (position, slot, target) in new[] { (0, 2, 3u), (0, 3, 4u), (2, 1, 2u), (1, 0, 0u) })
        {
            var count = core.PageEdits.Count;
            Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), position, DocumentBlockAction.Move, slot))).IsSuccess,
                "Valid block move must succeed");
            Check(core.PageEdits.Count == count + 1 && core.PageEdits.Last() == new PdfCoreEdit.MovePages(core.Blocks[position].Start, core.Blocks[position].Count, target),
                "A block move is one command using post-removal target positions");
        }
        var edits = core.PageEdits.Count;
        Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(snapshot, 0, DocumentBlockAction.Delete))).IsSuccess
            && core.PageEdits.Count == edits, "Old revision must not delete new page positions");
        foreach (var slot in new[] { -1, 1, 2, 4 })
            Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), 1, DocumentBlockAction.Move, slot))).IsSuccess,
                "Invalid and unchanged slots must record nothing");
        Check(core.PageEdits.Count == edits, "Rejected moves must not reach core");
        Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), 1, DocumentBlockAction.RotateLeft))).IsSuccess
            && core.PageEdits.Last() == new PdfCoreEdit.RotatePages(2, 3, -90), "Rotate left must be one range command");
        Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), 1, DocumentBlockAction.RotateRight))).IsSuccess
            && core.PageEdits.Last() == new PdfCoreEdit.RotatePages(2, 3, 90), "Rotate right must be one range command");
        Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), 1, DocumentBlockAction.Delete))).IsSuccess
            && core.PageEdits.Last() == new PdfCoreEdit.RemovePages(2, 3), "Delete must remove the whole range once");
        core.Blocks = [new(DocumentBlockSource.Base, null, 0, 3)];
        var empty = await facade.EditPagesAsync(session.SessionId, new PageEdit.Block(await Read(), 0, DocumentBlockAction.Delete));
        Check(empty.IsSuccess && empty.Value!.PageCount == 0, "Documents-view deletion must allow an empty undoable document");
        Check(!(await facade.DocumentBlocksAsync("stale")).IsSuccess, "Stale document must not expose blocks");
        await CheckPagesAsync();
    }

    private static async Task CheckPagesAsync()
    {
        var core = new FakeCore { PageCount = 3, Blocks = [new(DocumentBlockSource.Base, null, 0, 3)] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new("pages.pdf", [1]))).Value!;
        async Task<DocumentBlocksSnapshot> Read() => (await facade.DocumentBlocksAsync(session.SessionId)).Value!;
        var initial = await Read();
        Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(initial with { SessionId = "other" }, 0,
            DocumentBlockAction.Delete))).IsSuccess && core.PageEdits.Count == 0, "Another session must not target this page");
        foreach (var (from, to) in new[] { (0u, 2u), (2u, 0u), (1u, 2u) })
        {
            var count = core.PageEdits.Count;
            Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), from, DocumentBlockAction.Move, to))).IsSuccess
                && core.PageEdits.Count == count + 1 && core.PageEdits.Last() == new PdfCoreEdit.MovePages(from, 1, to),
                "One page move must be one command in post-removal coordinates");
        }
        var edits = core.PageEdits.Count;
        Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(initial, 0, DocumentBlockAction.Delete))).IsSuccess,
            "Stale page revision must be refused");
        foreach (var (from, to) in new[] { (0u, 0u), (0u, 3u), (3u, 0u) })
            Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), from, DocumentBlockAction.Move, to))).IsSuccess,
                "Out-of-range/no-op moves must be refused");
        Check(edits == core.PageEdits.Count, "Refused page edits must not reach core");
        Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), 1, DocumentBlockAction.RotateLeft))).IsSuccess
            && core.PageEdits.Last() == new PdfCoreEdit.RotatePages(1, 1, -90), "Left turn must target one page");
        Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), 1, DocumentBlockAction.RotateRight))).IsSuccess
            && core.PageEdits.Last() == new PdfCoreEdit.RotatePages(1, 1, 90), "Right turn must target one page");
        for (var remaining = 3; remaining > 0; remaining--)
            Check((await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), 0, DocumentBlockAction.Delete))).Value?.PageCount == remaining - 1,
                "Organize must allow deleting the final page");
        Check(!(await facade.EditPagesAsync(session.SessionId, new PageEdit.OrganizePage(await Read(), 0, DocumentBlockAction.Delete))).IsSuccess,
            "Empty document has no deletable page");
    }
}
