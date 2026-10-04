using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native harness. Rebuild the normal entry point afterward.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.OrganizeDocumentsSmokeAsync();
            File.WriteAllText(Path.Combine(output, "organize-documents-smoke.log"),
                "PASS Documents default; exclusive selector; real-core blocks/covers; move/rotate/delete as one undo step; stale snapshot; busy gates; empty deletion/undo/insertion; split source parts; original sample restored.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "organize-documents-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task OrganizeDocumentsSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        Check(Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Vitela sample.pdf", await File.ReadAllBytesAsync(SamplePath));
        await EnterOrganizeViewAsync();
        var id = _session!.SessionId;
        var original = _session.PageCount;
        async Task<DocumentBlocksSnapshot> Read() => (await _facade.DocumentBlocksAsync(id)).Value!;
        var initial = await Read();
        Check(_showingOrganizeDocuments && OrganizeDocumentsToggle.IsChecked == true && OrganizePagesToggle.IsChecked == false,
            "Organize must open on Documents");
        Check(initial.Blocks.Count == 1 && initial.Blocks[0].Source == DocumentBlockSource.Base && initial.Blocks[0].Count == original,
            "Initial base block must match core");
        await PopulateOrganizeDocumentsAsync();
        Check(_organizeDocumentActions.Count == 5 && !_organizeDocumentActions[0].Button.IsEnabled && !_organizeDocumentActions[1].Button.IsEnabled,
            "Single block boundary moves must be disabled");
        ShowOrganizeDocuments(false);
        Check(OrganizePagesToggle.IsChecked == true && OrganizeDocumentsToggle.IsChecked == false && _organizeCards.Count == original,
            "Pages selector must be exclusive and show page cards");
        ShowOrganizeDocuments(true);
        await InsertBlankPageAsync(original, PageOrientation.Portrait);
        var added = await Read();
        Check(added.Blocks.Count == 2 && added.Blocks[1].Source == DocumentBlockSource.Blank && added.Blocks[1].Count == 1,
            "Blank insertion must derive a second document block");
        async Task Edit(int position, DocumentBlockAction action, int slot = 0) =>
            await EditOrganizeBlockAsync(await Read(), position, action, slot, id, _organizeDocumentsGeneration);
        await Edit(1, DocumentBlockAction.Move, 0);
        Check((await Read()).Blocks[0].Source == DocumentBlockSource.Blank, "Move up must move the whole block");
        await RunOrganizeHistoryAsync(true);
        Check((await Read()).Blocks.SequenceEqual(added.Blocks), "One Undo must restore the block move");
        await RunOrganizeHistoryAsync(false);
        Check((await Read()).Blocks[0].Source == DocumentBlockSource.Blank, "Redo must restore the block move");
        await RunOrganizeHistoryAsync(true);
        var stale = await _facade.EditPagesAsync(id, new PageEdit.Block(initial, 0, DocumentBlockAction.Delete));
        Check(!stale.IsSuccess && _session!.PageCount == original + 1, "Stale snapshot must not delete current pages");
        var dimensions = _session!.Pages.Take((int)original).ToArray();
        await Edit(0, DocumentBlockAction.RotateRight);
        Check(_session!.Pages.Take((int)original).All(page => page.Rotation == PageRotation.Clockwise90), "Every page in a block must turn");
        await RunOrganizeHistoryAsync(true);
        Check(_session!.Pages.Take((int)original).SequenceEqual(dimensions), "One Undo must restore all block angles");
        await Edit(0, DocumentBlockAction.RotateLeft);
        Check(_session!.Pages.Take((int)original).All(page => page.Rotation == PageRotation.Clockwise270), "Left turn must rotate the entire block");
        await RunOrganizeHistoryAsync(true);
        SetBusy(true);
        Check(_organizeDocumentActions.All(action => !action.Button.IsEnabled) && !OrganizePagesToggle.IsEnabled, "Busy must disable keyboard actions and switching");
        SetBusy(false);
        await Edit(0, DocumentBlockAction.Delete);
        Check(_session!.PageCount == 1 && (await Read()).Blocks.Single().Source == DocumentBlockSource.Blank, "Delete must remove the entire base block");
        await RunOrganizeHistoryAsync(true);
        Check(_session!.PageCount == original + 1 && (await Read()).Blocks.SequenceEqual(added.Blocks), "One Undo must restore deleted block");
        await RunOrganizeHistoryAsync(true); // Undo insertion, leaving only the original block.
        Check(_session!.PageCount == original, "Insertion Undo must restore original sample");
        await Edit(0, DocumentBlockAction.Delete);
        Check(_session!.PageCount == 0 && (await Read()).Blocks.Count == 0 && _organizeDocumentActions.Count == 0,
            "Deleting the last block must leave an empty undoable Documents view");
        await RunOrganizeHistoryAsync(true);
        Check(_session!.PageCount == original && (await Read()).Blocks.SequenceEqual(initial.Blocks), "Undo must recover from zero pages");
        await Edit(0, DocumentBlockAction.Delete);
        await InsertBlankPageAsync(0, PageOrientation.Portrait);
        Check(_session!.PageCount == 1 && (await Read()).Blocks.Single().Source == DocumentBlockSource.Blank,
            "Blank insertion must recover an empty document");
        await RunOrganizeHistoryAsync(true);
        await RunOrganizeHistoryAsync(true);
        Check(_session!.PageCount == original, "Original sample must survive empty/insertion Undo");
        if (original > 1)
        {
            await InsertBlankPageAsync(1, PageOrientation.Portrait);
            var split = await Read();
            Check(split.Blocks.Count == 3 && split.Blocks[0].Part == 1 && split.Blocks[2].Part == 2,
                "Core split sources must expose Part 1/Part 2");
            await RunOrganizeHistoryAsync(true);
        }
        Check(_session!.PageCount == original && (await Read()).Blocks.SequenceEqual(initial.Blocks), "Final sample must be unchanged");
    }
}
