using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Windows.Storage;

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
            await _window.ImportPdfsSmokeAsync();
            File.WriteAllText(Path.Combine(output, "import-pdfs-smoke.log"),
                "PASS real-core multi-file import; one undo step per file; blocks named after their files; imported pages render; "
                + "encrypted source asks for and takes its password; save round-trips every imported page.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "import-pdfs-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ImportPdfsSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        Check(Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Vitela sample.pdf", await File.ReadAllBytesAsync(SamplePath));
        await EnterOrganizeViewAsync();
        var id = _session!.SessionId;
        var original = _session.PageCount;
        Check(OrganizeAddPdfsButton.IsEnabled, "Add PDFs must be available on an editable document");

        var sample = await StorageFile.GetFileFromPathAsync(SamplePath);
        var first = (await _facade.DocumentBlocksAsync(id)).Value!;
        await ImportPdfsAsync(id, [sample, sample]);
        Check(_session!.PageCount == original * 3, $"Both files' pages must be appended, got {_session.PageCount}");
        Check(AnnotationStatus.Text.Contains("from 2 PDFs"), $"Status must report both files: {AnnotationStatus.Text}");
        Check(ImportProgressPanel.Visibility == Visibility.Collapsed && OrganizeAddPdfsButton.IsEnabled, "Progress must clear when done");
        var blocks = (await _facade.DocumentBlocksAsync(id)).Value!.Blocks;
        Check(blocks.Count == 3 && blocks.Skip(1).All(block => block.Source == DocumentBlockSource.Imported),
            "Each file must be its own imported block");
        Check(blocks[1].ImportedSourceId < blocks[2].ImportedSourceId,
            "Files must land in pick order: the second appends after the first, not before it");
        Check(first.Blocks.Count == 1, "The sample must start as one base block");
        Check(blocks.Skip(1).All(block => ImportedBlockName(id, block.ImportedSourceId) == sample.Name),
            "Imported blocks must be named after the picked file");
        // The Documents view asks for this block's cover too, and a newer
        // render of a page discards an older one: retry until ours is kept.
        var render = await _facade.RenderPageAsync(id, original * 2, 72, invertContentColors: false);
        for (var attempt = 0; render.IsDiscarded && attempt < 20; attempt++)
            render = await _facade.RenderPageAsync(id, original * 2, 72, invertContentColors: false);
        Check(render.IsSuccess, $"An imported page must render from the rebuilt preview: {render.Error?.Message} empty={render.IsEmpty} discarded={render.IsDiscarded}");

        await RunOrganizeHistoryAsync(undo: true);
        Check(_session!.PageCount == original * 2, "One undo must take back exactly one file");
        await RunOrganizeHistoryAsync(undo: false);
        Check(_session!.PageCount == original * 3, "Redo must bring the file back from the handle's kept source");

        var aes = await File.ReadAllBytesAsync(Aes128SamplePath);
        var locked = await _facade.ImportPdfAsync(id, aes, null, _session.PageCount);
        Check(!locked.IsSuccess && locked.Error!.RequiresPassword, "A locked source must ask for its password");
        var wrong = await _facade.ImportPdfAsync(id, aes, "wrong", _session.PageCount);
        Check(!wrong.IsSuccess && wrong.Error!.RequiresPassword, "A wrong source password must ask again");
        var unlocked = await _facade.ImportPdfAsync(id, aes, "user-aes-pass", _session.PageCount);
        Check(unlocked.IsSuccess, "The source's user password must import it: " + unlocked.Error?.Message);
        var expected = unlocked.Value!.Session.PageCount;

        byte[]? saved = null;
        var save = await _facade.SaveToDestinationAsync(id, bytes => { saved = bytes; return Task.CompletedTask; });
        Check(save.IsSuccess && saved is not null, "Saving with imported pages must succeed: " + save.Error?.Message);
        var reopened = await _facade.OpenAsync(new DocumentSource("saved.pdf", saved!), discardPendingEdits: true);
        Check(reopened.IsSuccess && reopened.Value!.PageCount == expected, "The saved file must hold every imported page");
    }
}
