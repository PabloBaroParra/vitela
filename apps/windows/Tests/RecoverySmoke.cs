using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Recovery;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>
/// Crash recovery end to end on the real core and the real window: edit, force
/// a snapshot, crash (release the entry without cleanup), "relaunch" (a fresh
/// scheduler offering what the scan finds), Restore through the real dialog,
/// and check the restored document is the edited one. Never touches the user's
/// recovery directory: everything lives under VITELA_SMOKE_OUTPUT.
/// </summary>
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
        var log = Path.Combine(output, "recovery-smoke.log");
        try
        {
            File.WriteAllText(log, "PASS " + await _window.RecoverySmokeAsync(output));
        }
        catch (Exception error) { File.WriteAllText(log, "FAIL " + error); }
        finally { _window.CloseAfterRecoverySmoke(); }
    }
}

public sealed partial class MainWindow
{
    internal void CloseAfterRecoverySmoke()
    {
        _approvedClose = true;
        Close();
    }

    internal async Task<string> RecoverySmokeAsync(string output)
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
        {
            yield return parent;
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(parent); i++)
                foreach (var child in Descendants(VisualTreeHelper.GetChild(parent, i))) yield return child;
        }
        async Task Wait(Func<bool> predicate, string message)
        {
            for (var i = 0; i < 400 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        ContentDialog? CurrentDialog() => Content.XamlRoot is { } root
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(popup => Descendants(popup.Child)).OfType<ContentDialog>().FirstOrDefault()
            : null;
        static void Click(ContentDialog dialog, string name)
        {
            var button = Descendants(dialog).OfType<Button>().Single(button => button.Name == name);
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(button);
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }
        string[] Entries(RecoveryDirectory dir, string extension) =>
            Directory.Exists(dir.Root) ? Directory.GetFiles(dir.Root, "*." + extension) : [];

        await Wait(() => Content.XamlRoot is not null, "Window did not load");
        var scratch = Path.Combine(output, "recovery-smoke-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(scratch);
        var dir = new RecoveryDirectory(Path.Combine(scratch, "recovery"));
        var original = Path.Combine(scratch, "quote.pdf");
        File.Copy(SamplePath, original);
        var title = "Recovery smoke " + Guid.NewGuid().ToString("N")[..8];

        // A real file, so the document has an origin to restore to.
        await OpenStorageFileAsync(await StorageFile.GetFileFromPathAsync(original));
        Check(_session?.DisplayName == "quote.pdf" && _documentOrigin == original, "the copied sample opens with its origin");
        var info = (await _facade.DocumentInfoAsync(_session!.SessionId)).Value!;
        Check((await _facade.SetDocumentInfoAsync(_session.SessionId, info with { Title = title })).IsSuccess, "fixture edit");
        var pages = _session.PageCount;

        // First launch: autosave on, nothing to offer yet.
        EnableCrashRecovery(dir);
        Check(CurrentDialog() is null, "an empty recovery directory offers nothing");
        await TickAutosaveAsync();
        Check(Entries(dir, "pdf").Length == 1 && Entries(dir, "meta").Length == 1, "a forced tick writes one entry for the unsaved edit");
        Check(dir.Orphans(_ => { }).Count == 0, "while the window lives, its entry is held and not offered");
        var crashedPdf = Entries(dir, "pdf").Single();
        var unchangedTick = _autosave!.Attempts;
        await TickAutosaveAsync();
        Check(_autosave.Attempts == unchangedTick, "an unchanged document is not serialized again");

        // Crash: the lock goes with the process, the files stay.
        _autosave.ReleaseLikeACrash();
        _autosaveTimer!.Stop();
        Check(File.Exists(crashedPdf), "a crash leaves the entry");

        // Relaunch: an empty window, a fresh scheduler, the startup offer.
        var blank = await _facade.CreateBlankAsync(discardPendingEdits: true);
        ShowOpenedDocument(blank.Value!);
        _autosave = null;
        _autosaveTimer = null;
        _recoveryOffered = false;
        EnableCrashRecovery(dir);
        await Wait(() => CurrentDialog()?.Title?.ToString() == "Recover unsaved work?", "the relaunch offers the crashed entry");
        var dialog = CurrentDialog()!;
        Check(((TextBlock)dialog.Content).Text.Contains("quote.pdf"), "the offer names the document");
        await Wait(() => Descendants(dialog).OfType<Button>().Any(button => button.Name == "PrimaryButton"), "dialog buttons load");
        Click(dialog, "PrimaryButton");

        await Wait(() => _session?.DisplayName == "quote.pdf" && !_isBusy, "Restore reopens the crashed document");
        var restored = _session!;
        Check(_documentOrigin == original, "with its origin, which Save pre-selects");
        Check(restored.PageCount == pages, "with every page");
        Check((await _facade.DocumentInfoAsync(restored.SessionId)).Value!.Title == title, "with the unsaved edit in it");
        Check((await _facade.HasUnsavedChangesAsync(restored.SessionId)).Value, "as an unsaved document");
        await Wait(() => !File.Exists(crashedPdf), "the crashed entry is deleted once this window has written its own");
        Check(Entries(dir, "pdf").Length == 1 && Entries(dir, "pdf").Single() != crashedPdf, "the restored work is in this window's entry");

        // A clean close deletes everything.
        ShutDownAutosave();
        Check(Entries(dir, "pdf").Length == 0 && Entries(dir, "meta").Length == 0 && Entries(dir, "lock").Length == 0,
            "a clean shutdown leaves nothing to offer");
        Check(File.ReadAllBytes(original).SequenceEqual(File.ReadAllBytes(SamplePath)), "the original file was never written");
        Directory.Delete(scratch, recursive: true);
        return "edit -> forced snapshot (held, not re-serialized) -> crash -> relaunch offer -> Restore: name, origin, pages, unsaved metadata edit, unsaved state; crashed entry replaced by own; clean shutdown empties the directory; original untouched.";
    }
}
