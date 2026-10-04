using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Media;
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
                "PASS real-core multi-file import; one undo/redo for the whole batch; cancelled and failed preparation preserve history; "
                + "warning confirmation defaults to Cancel; blocks named after their files; imported pages render; "
                + "encrypted source password cancel/retry; save round-trips every imported page; "
                + "empty destination recovery with one undo/redo; real encrypted destination refusal before picker preserves pages and history.");
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
        static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
        {
            yield return parent;
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
                foreach (var child in Descendants(VisualTreeHelper.GetChild(parent, i))) yield return child;
        }
        async Task<ContentDialog> Dialog(string title)
        {
            for (var i = 0; i < 200; i++)
            {
                var dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                    .SelectMany(popup => Descendants(popup.Child)).OfType<ContentDialog>()
                    .FirstOrDefault(dialog => dialog.Title?.ToString() == title);
                if (dialog is not null && Descendants(dialog).OfType<Button>().Any(button => button.Name == "PrimaryButton")) return dialog;
                await Task.Delay(25);
            }
            throw new InvalidOperationException("Missing modal: " + title);
        }
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(button => button.Name == name);
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button);
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        Check(Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Vitela sample.pdf", await File.ReadAllBytesAsync(SamplePath));
        await EnterOrganizeViewAsync();
        var id = _session!.SessionId;
        var original = _session.PageCount;
        Check(OrganizeAddPdfsButton.IsEnabled, "Add PDFs must be available on an editable document");

        var sample = await StorageFile.GetFileFromPathAsync(SamplePath);
        var first = (await _facade.DocumentBlocksAsync(id)).Value!;
        var checking = await _facade.PrepareImportAsync(await File.ReadAllBytesAsync(SamplePath), null);
        Check(checking.IsSuccess, "Sample must prepare");
        using var sampleSource = checking.Value!;
        var importing = ImportPdfsAsync(id, [sample, sample]);
        if (sampleSource.Warnings.Count > 0)
        {
            var warning = await Dialog("Some document-level information will stay behind");
            Check(_session.PageCount == original, "Warnings must be shown before any pages are added");
            Click(warning, "PrimaryButton");
        }
        await importing;
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
        Check(_session!.PageCount == original && !OrganizeUndoButton.IsEnabled,
            "One undo must take back the whole batch without leaving a second import undo");

        var revision = (await _facade.DocumentBlocksAsync(id)).Value!.Revision;
        importing = ImportPdfsAsync(id, [sample, sample]);
        CancelImportButton_Click(CancelImportButton, new RoutedEventArgs());
        await importing;
        Check(_session.PageCount == original && AnnotationStatus.Text == ImportCancelled
            && (await _facade.DocumentBlocksAsync(id)).Value!.Revision == revision && OrganizeRedoButton.IsEnabled,
            "Cancelling preparation must preserve pages, revision and redo");

        var invalidPath = Path.Combine(Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!, "import-invalid.pdf");
        await File.WriteAllBytesAsync(invalidPath, [1, 2, 3]);
        var invalid = await StorageFile.GetFileFromPathAsync(invalidPath);
        await ImportPdfsAsync(id, [sample, invalid]);
        Check(_session.PageCount == original && AnnotationStatus.Text.StartsWith("Could not import import-invalid.pdf:")
            && (await _facade.DocumentBlocksAsync(id)).Value!.Revision == revision && OrganizeRedoButton.IsEnabled,
            "A later preparation failure must discard earlier sources and preserve redo");

        var confirmation = ConfirmImportWarningsAsync(["fixture.pdf: document information will be omitted"]);
        var confirmationDialog = await Dialog("Some document-level information will stay behind");
        Check(confirmationDialog.DefaultButton == ContentDialogButton.Close, "Warning confirmation must default to Cancel");
        Click(confirmationDialog, "CloseButton");
        Check(!await confirmation && _session.PageCount == original, "Warning cancellation must add no pages");
        confirmation = ConfirmImportWarningsAsync(["fixture.pdf: document information will be omitted"]);
        Click(await Dialog("Some document-level information will stay behind"), "PrimaryButton");
        Check(await confirmation, "Import anyway must explicitly acknowledge warnings");

        await RunOrganizeHistoryAsync(undo: false);
        Check(_session!.PageCount == original * 3, "One redo must bring the whole batch back from the kept sources");

        var aes = await File.ReadAllBytesAsync(Aes128SamplePath);
        var encryptedFile = await StorageFile.GetFileFromPathAsync(Aes128SamplePath);
        importing = ImportPdfsAsync(id, [sample, encryptedFile]);
        Click(await Dialog("Password required"), "CloseButton");
        await importing;
        Check(_session.PageCount == original * 3 && AnnotationStatus.Text == ImportCancelled,
            "Cancelling a later source password must discard the earlier prepared file");

        var preparing = PrepareImportAsync(id, encryptedFile.Name, aes);
        var passwordDialog = await Dialog("Password required");
        var passwordBox = ((StackPanel)passwordDialog.Content).Children.OfType<PasswordBox>().Single();
        passwordBox.Password = "wrong";
        Click(passwordDialog, "PrimaryButton");
        passwordDialog = await Dialog("Password required");
        Check(((StackPanel)passwordDialog.Content).Children.OfType<TextBlock>().Any(text => text.Text == "The password is incorrect. Try again."),
            "A wrong source password must ask again");
        passwordBox = ((StackPanel)passwordDialog.Content).Children.OfType<PasswordBox>().Single();
        passwordBox.Password = "user-aes-pass";
        Click(passwordDialog, "PrimaryButton");
        var prepared = await preparing;
        Check(prepared is { IsSuccess: true }, "The source's user password must prepare it");
        using var unlockedSource = prepared!.Value!;
        var unlocked = await _facade.ImportPreparedAsync(id, [unlockedSource], _session.PageCount);
        Check(unlocked.IsSuccess, "Prepared encrypted source must import: " + unlocked.Error?.Message);
        var expected = unlocked.Value!.Session.PageCount;

        byte[]? saved = null;
        var save = await _facade.SaveToDestinationAsync(id, bytes => { saved = bytes; return Task.CompletedTask; });
        Check(save.IsSuccess && saved is not null, "Saving with imported pages must succeed: " + save.Error?.Message);
        var reopened = await _facade.OpenAsync(new DocumentSource("saved.pdf", saved!), discardPendingEdits: true);
        Check(reopened.IsSuccess && reopened.Value!.PageCount == expected, "The saved file must hold every imported page");

        // Start from a real zero-page session, not a shell-only PageCount override.
        ShowOpenedDocument(reopened.Value!);
        await EnterOrganizeViewAsync();
        id = _session!.SessionId;
        var savedBlocks = (await _facade.DocumentBlocksAsync(id)).Value!;
        Check(savedBlocks.Blocks.Count == 1, "Reopened pages must form one base block");
        await EditOrganizeBlockAsync(savedBlocks, 0, DocumentBlockAction.Delete, 0, id, _organizeDocumentsGeneration);
        var empty = (await _facade.DocumentBlocksAsync(id)).Value!;
        Check(_session!.PageCount == 0 && empty.Blocks.Count == 0 && _organizeDocumentActions.Count == 0,
            "Deleting the final block must leave a real empty Organize view");
        Check(OrganizeAddPdfsButton.IsEnabled && OrganizeUndoButton.IsEnabled && !OrganizeSaveButton.IsEnabled,
            "An empty destination must permit import and recovery Undo, but not Save");
        Check((await _facade.ImportRefusalAsync(id)) is { IsSuccess: true, Value: null },
            "Zero pages must not itself refuse importing");

        // No files is cancellation, even when the destination has no pages.
        await ImportPdfsAsync(id, []);
        Check(_session.PageCount == 0 && AnnotationStatus.Text == ImportCancelled
            && (await _facade.DocumentBlocksAsync(id)).Value!.Revision == empty.Revision
            && OrganizeAddPdfsButton.IsEnabled && ImportProgressPanel.Visibility == Visibility.Collapsed,
            "An empty pick must preserve the empty destination and restore import controls");
        importing = ImportPdfsAsync(id, [sample, sample]);
        if (sampleSource.Warnings.Count > 0) Click(await Dialog("Some document-level information will stay behind"), "PrimaryButton");
        await importing;
        var recovered = (await _facade.DocumentBlocksAsync(id)).Value!;
        Check(_session.PageCount == original * 2 && recovered.Blocks.Count == 2
            && recovered.Blocks.All(block => block.Source == DocumentBlockSource.Imported)
            && recovered.Revision == empty.Revision + 1 && OrganizeSaveButton.IsEnabled,
            "One batch must recover all picked pages from an empty destination in one revision");
        await RunOrganizeHistoryAsync(undo: true);
        Check(_session.PageCount == 0 && (await _facade.DocumentBlocksAsync(id)).Value!.Blocks.Count == 0
            && OrganizeRedoButton.IsEnabled && OrganizeAddPdfsButton.IsEnabled && !OrganizeSaveButton.IsEnabled,
            "One Undo must restore the empty destination and its recovery controls");
        await RunOrganizeHistoryAsync(undo: false);
        Check(_session.PageCount == original * 2
            && (await _facade.DocumentBlocksAsync(id)).Value!.Blocks.SequenceEqual(recovered.Blocks)
            && OrganizeSaveButton.IsEnabled && ImportProgressPanel.Visibility == Visibility.Collapsed,
            "One Redo must restore the complete imported batch and Save availability");

        // This shipped fixture grants copying but denies content changes and
        // page assembly when opened with its user password. Use the real core.
        var restricted = await _facade.OpenAsync(new DocumentSource("restricted.pdf", aes), "user-aes-pass", discardPendingEdits: true);
        Check(restricted.IsSuccess, "The restricted destination must open with its user password");
        ShowOpenedDocument(restricted.Value!);
        await EnterOrganizeViewAsync();
        id = _session!.SessionId;
        var restrictedBlocks = (await _facade.DocumentBlocksAsync(id)).Value!;
        var refusal = await _facade.ImportRefusalAsync(id);
        Check(refusal.IsSuccess && refusal.Value == "This document does not permit changing its content.",
            "The real encrypted fixture must refuse content changes, not merely fail a rewrite");
        Check(OrganizeAddPdfsButton.IsEnabled && !OrganizeUndoButton.IsEnabled && !OrganizeRedoButton.IsEnabled,
            "The import action must remain available to explain the core's refusal");
        OrganizeAddPdfsButton_Click(OrganizeAddPdfsButton, new RoutedEventArgs());
        for (var attempt = 0; _organizeBusy && attempt < 200; attempt++) await Task.Delay(25);
        var afterRefusal = (await _facade.DocumentBlocksAsync(id)).Value!;
        Check(!_organizeBusy && !_isBusy && AnnotationStatus.Text == refusal.Value,
            "A refused import must report the core's reason before opening a picker and release busy ownership");
        Check(_session.SessionId == id && _session.PageCount == restricted.Value!.PageCount
            && afterRefusal.Revision == restrictedBlocks.Revision && afterRefusal.Blocks.SequenceEqual(restrictedBlocks.Blocks)
            && !OrganizeUndoButton.IsEnabled && !OrganizeRedoButton.IsEnabled
            && !(await _facade.HasUnsavedChangesAsync(id)).Value,
            "Refusal must preserve the restricted destination's pages, revision and clean history");
        Check(OrganizeAddPdfsButton.IsEnabled && OrganizeReturnButton.IsEnabled
            && ImportProgressPanel.Visibility == Visibility.Collapsed && !_dialogOpen,
            "Refusal must leave no progress/modal and restore usable Organize controls");
        using var permittedSource = (await _facade.PrepareImportAsync(await File.ReadAllBytesAsync(SamplePath), null)).Value!;
        var refusedApply = await _facade.ImportPreparedAsync(id, [permittedSource], _session.PageCount);
        Check(!refusedApply.IsSuccess && refusedApply.Error!.Message == refusal.Value
            && (await _facade.DocumentBlocksAsync(id)).Value!.Revision == restrictedBlocks.Revision,
            "The mutation boundary must enforce the same permission refusal if called directly");
    }
}
