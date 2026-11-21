using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.Storage;

namespace Pdf.Windows;

/// <summary>
/// Document tabs end to end on the real core and the real window: several
/// files opened as one action (first in front, the rest behind), switching
/// with the strip and its reading position, a reopen that switches or
/// reloads, Save As refused onto another tab's file, per-tab unsaved prompts
/// on tab close, and the last close returning to Home. Files live under
/// VITELA_SMOKE_OUTPUT; nothing is written to them.
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
        var log = Path.Combine(output, "tabs-smoke.log");
        try
        {
            File.WriteAllText(log, "PASS " + await _window.TabsSmokeAsync(output));
        }
        catch (Exception error) { File.WriteAllText(log, "FAIL " + error); }
        finally { _window.CloseAfterTabsSmoke(); }
    }
}

public sealed partial class MainWindow
{
    internal void CloseAfterTabsSmoke()
    {
        _approvedClose = true;
        if (!_windowClosed) Close();
    }

    internal async Task<string> TabsSmokeAsync(string output)
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
            for (var i = 0; i < 200 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        ContentDialog? CurrentDialog() => Content.XamlRoot is { } root
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(popup => Descendants(popup.Child)).OfType<ContentDialog>().FirstOrDefault()
            : null;
        async Task Answer(string title, string button)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == title, "Missing modal: " + title);
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(b => b.Name == button), "Modal buttons not loaded");
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(Descendants(dialog).OfType<Button>().Single(b => b.Name == button));
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }
        async Task Strip()
        {
            // The strip redraws on the dispatcher after every change; let it run.
            await Wait(() => !_tabStripSyncQueued, "Strip redraw did not run");
        }

        await Wait(() => _shellInitialized && Content.XamlRoot is not null, "Shell not loaded");
        var scratch = Path.Combine(output, "tabs-smoke-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(scratch);
        string Copy(string name)
        {
            var path = Path.Combine(scratch, name);
            File.Copy(SamplePath, path);
            return path;
        }
        var first = Copy("first.pdf");
        var second = Copy("second.pdf");
        var third = Copy("third.pdf");

        // A failed first file with nothing on screen: the next one lands in front.
        Check(TabStrip.Visibility == Visibility.Collapsed, "No strip before any document");
        var broken = Path.Combine(scratch, "broken.pdf");
        File.WriteAllBytes(broken, [1, 2, 3]);
        var good = Copy("good.pdf");
        await OpenPathsAsync([broken, good]);
        Check(_tabList.Count == 1 && _tabList.Active is not null && _session?.DisplayName == "good.pdf"
            && EditorView.Visibility == Visibility.Visible && PageScroller.Visibility == Visibility.Visible,
            "After a failed first file the second is the active tab, on screen");
        await RequestCloseTabAsync(_tabList.Active!.Value);
        Check(_tabList.IsEmpty && _session is null, "Back to no document");

        // One action, three files: the first in front, the rest behind it.
        await OpenPathsAsync([first, second, third]);
        await Strip();
        Check(_tabList.Count == 3 && _session?.DisplayName == "first.pdf" && _documentOrigin == first, "First file lands in front");
        Check(_tabList.Ids[0] == _tabList.Active, "The front tab is the first in strip order");
        Check(TabStripPanel.Children.Count == 3 && TabStrip.Visibility == Visibility.Visible, "Strip shows a tab per document");
        Check(Title == "first.pdf — Vitela", "Window title names the active document");
        var firstSession = _session!.SessionId;

        // Edit the front tab, then switch: the edit stays with its tab.
        var info = (await _facade.DocumentInfoAsync(firstSession)).Value!;
        Check((await _facade.SetDocumentInfoAsync(firstSession, info with { Title = "Tabs smoke" })).IsSuccess, "fixture edit");
        await RequestTabActivationAsync(_tabList.Ids[1]);
        Check(_session?.DisplayName == "second.pdf" && _documentOrigin == second && _tabList.Active == _tabList.Ids[1], "Strip click switches tabs");
        Check(!(await _facade.HasUnsavedChangesAsync(_session!.SessionId)).Value, "The second tab is clean: the edit stayed behind");
        await Strip();
        Check(_tabs[_tabList.Ids[0]].Dirty && !_tabs[_tabList.Ids[1]].Dirty, "Strip marks only the dirty tab");

        // Reopening a file a background tab holds switches to it.
        await OpenPathsAsync([first]);
        Check(_session?.SessionId == firstSession && _tabList.Count == 3, "Reopen of a held file switches, never opens twice");
        Check((await _facade.HasUnsavedChangesAsync(firstSession)).Value, "The switched-to tab kept its unsaved edit");

        // Reopening the dirty file on screen asks first; Cancel keeps the work.
        var reload = OpenPathsAsync([first]);
        await Answer("Unsaved changes", "CloseButton");
        await reload;
        Check(_session?.SessionId == firstSession && (await _facade.HasUnsavedChangesAsync(firstSession)).Value, "Reload Cancel keeps the work");
        // Discard reloads it from disk, in the same tab.
        reload = OpenPathsAsync([first]);
        await Answer("Unsaved changes", "SecondaryButton");
        await reload;
        Check(_session?.SessionId != firstSession && _tabList.Count == 3 && _documentOrigin == first
            && !(await _facade.HasUnsavedChangesAsync(_session!.SessionId)).Value, "Reload Discard reads the file again in its tab");
        // A clean file on screen just reloads.
        var reloaded = _session!.SessionId;
        await OpenPathsAsync([first]);
        Check(_session?.SessionId != reloaded && _tabList.Count == 3 && CurrentDialog() is null, "Clean reload asks nothing");

        // Two tabs never claim one file.
        Check(RefuseHeldDestination(second) && AnnotationStatus.Text == "That file is open in another tab. Close that tab first, or choose another name.",
            "Save As onto another tab's file is refused");
        Check(!RefuseHeldDestination(first) && !RefuseHeldDestination(Path.Combine(scratch, "new.pdf")), "Its own file or a free one is allowed");

        // Ctrl+Tab order wraps around the strip.
        Check(_tabList.Cycle(+1) == _tabList.Ids[1] && _tabList.Cycle(-1) == _tabList.Ids[2], "Cycling wraps around");

        // A busy window refuses the switch at once.
        SetBusy(true);
        await RequestTabActivationAsync(_tabList.Ids[2]);
        SetBusy(false);
        Check(_session?.DisplayName == "first.pdf" && AnnotationStatus.Text.StartsWith("Wait for the current operation"), "Busy refuses a switch");

        // A clean background tab closes without a question.
        await RequestCloseTabAsync(_tabList.Ids[2]);
        Check(_tabList.Count == 2 && _session?.DisplayName == "first.pdf" && CurrentDialog() is null, "Clean background tab closes quietly");

        // A dirty active tab asks: Cancel keeps it, Discard closes it.
        var dirtySession = _session!.SessionId;
        info = (await _facade.DocumentInfoAsync(dirtySession)).Value!;
        Check((await _facade.SetDocumentInfoAsync(dirtySession, info with { Title = "Close me" })).IsSuccess, "second fixture edit");
        var closing = RequestCloseTabAsync(_tabList.Active!.Value);
        await Wait(() => CurrentDialog() is { } d && ((TextBlock)d.Content).Text.EndsWith("Closing it will discard them."), "Tab close has its own explanation");
        await Answer("Unsaved changes", "CloseButton");
        await closing;
        Check(_tabList.Count == 2 && _session?.SessionId == dirtySession, "Tab close Cancel keeps the tab");
        closing = RequestCloseTabAsync(_tabList.Active!.Value);
        await Answer("Unsaved changes", "SecondaryButton");
        await closing;
        Check(_tabList.Count == 1 && _session?.DisplayName == "second.pdf", "Tab close Discard hands over to the neighbour");

        // A blank document lands in a tab of its own; closing everything returns Home.
        await CreateNewDocumentAsync();
        Check(_tabList.Count == 2 && _session?.PageCount == 1 && _documentOrigin is null, "New lands in its own tab");
        await RequestCloseTabAsync(_tabList.Active!.Value);
        await RequestCloseTabAsync(_tabList.Active!.Value);
        await Strip();
        Check(_tabList.IsEmpty && _session is null && HomeView.Visibility == Visibility.Visible
            && TabStrip.Visibility == Visibility.Collapsed && Title == "Vitela", "Closing the last tab returns Home");

        // The window still opens after its last tab closed.
        await OpenPathsAsync([third]);
        Check(_tabList.Count == 1 && _session?.DisplayName == "third.pdf", "A fresh open after Home lands in a new first tab");
        // A page-order edit survives a switch: Undo after coming back resyncs the viewer.
        var pagesBefore = _session!.PageCount;
        await EnterOrganizeViewAsync();
        await EditPagesAsync(new PageEdit.InsertBlank(0, PageOrientation.Portrait), "added", _ => { });
        LeaveOrganizeView();
        Check(_session!.PageCount == pagesBefore + 1, "Organize inserted a page");
        var organized = _tabList.Active!.Value;
        await CreateNewDocumentAsync();
        await RequestTabActivationAsync(organized);
        Check(_session?.DisplayName == "third.pdf" && _session.PageCount == pagesBefore + 1, "Switched back to the organized tab");
        await ApplyHistoryAsync(undo: true);
        var core = (await _facade.SessionAsync(_session!.SessionId)).Value!;
        Check(core.PageCount == pagesBefore && _session.PageCount == core.PageCount && _slots.Count == (int)core.PageCount,
            "Undo after a switch resyncs the viewer's pages with the core");

        foreach (var file in new[] { first, second, third })
            Check(File.ReadAllBytes(file).SequenceEqual(File.ReadAllBytes(SamplePath)), "No file was written");
        return "three files as one action (front + background), strip switch keeps per-tab edits and dirty marks, held-file reopen switches, "
            + "active reopen asks Cancel/Discard then reloads clean, Save As onto another tab refused, cycle wraps, busy refuses, "
            + "clean background close, dirty close Cancel/Discard with neighbour hand-over, New in its own tab, last close returns Home, "
            + "failed first file still lands the next one in front, page edit Undo after a switch resyncs the viewer.";
    }
}
