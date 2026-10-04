using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

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
            await _window.DocumentStatesSmokeAsync();
            File.WriteAllText(Path.Combine(output, "document-states-smoke.log"),
                "PASS startup/no-page/busy/permission gates, busy restoration, failed-open preservation, real full Undo/New Cancel/Discard and fresh clean replacement.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "document-states-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task DocumentStatesSmokeAsync()
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
            ? VisualTreeHelper.GetOpenPopupsForXamlRoot(root).SelectMany(p => Descendants(p.Child)).OfType<ContentDialog>().FirstOrDefault()
            : null;
        async Task ClickDecision(string name)
        {
            await Wait(() => CurrentDialog()?.Title?.ToString() == "Unsaved changes", "Missing full-Undo prompt");
            var dialog = CurrentDialog()!;
            await Wait(() => Descendants(dialog).OfType<Button>().Any(b => b.Name == name), "Modal not loaded");
            var peer = FrameworkElementAutomationPeer.CreatePeerForElement(Descendants(dialog).OfType<Button>().Single(b => b.Name == name));
            ((IInvokeProvider)peer.GetPattern(PatternInterface.Invoke)).Invoke();
        }
        await Wait(() => _shellInitialized, "Shell not loaded");
        Check(!SaveButton.IsEnabled && !PrintButton.IsEnabled && !ZoomInButton.IsEnabled && !FitWidthButton.IsEnabled
            && !UndoButton.IsEnabled && !HighlightButton.IsEnabled, "Startup document commands must be disabled");
        await OpenDocumentAsync("invalid.pdf", [1, 2, 3]);
        Check(_session is null && ErrorState.Visibility == Visibility.Visible && !SaveButton.IsEnabled, "Initial failure view");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        await RefreshAnnotationStateAsync();
        var session = _session!;
        Check(SaveButton.IsEnabled && HighlightButton.IsEnabled && ZoomInButton.IsEnabled, "Opened command gates");
        await OpenDocumentAsync("invalid.pdf", [1, 2, 3]);
        Check(_session!.SessionId == session.SessionId && PageScroller.Visibility == Visibility.Visible
            && ErrorState.Visibility == Visibility.Collapsed && HighlightButton.IsEnabled, "Failed replacement preserves document/tools");
        var state = _annotationState!;
        _annotationState = state with { EditingAllowed = false };
        SetBusy(true);
        Check(!SaveButton.IsEnabled && !HighlightButton.IsEnabled && !ContentEditButton.IsEnabled, "Busy gates");
        SetBusy(false);
        Check(!HighlightButton.IsEnabled && ContentEditButton.IsEnabled, "Permission bits remain independent after busy");
        _annotationState = state;
        SetBusy(false);
        Check(HighlightButton.IsEnabled, "Busy release restores annotation snapshot");
        _session = session with { PageCount = 0, State = DocumentSessionState.Empty, Pages = [] };
        ShowEmpty("The PDF contains no pages.");
        UpdateAnnotationControls(state);
        Check(!SaveButton.IsEnabled && !PrintButton.IsEnabled && !ZoomInButton.IsEnabled && !FitPageButton.IsEnabled
            && !HighlightButton.IsEnabled && !ContentEditButton.IsEnabled && EmptyStateMessage.Text == "The PDF contains no pages.", "Pageless gates and message");
        _session = session;
        EmptyState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Visible;
        SetBusy(false);
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Highlight, 0,
            new PdfCoreRect(10, 20, 30, 40), new PdfCoreColor(255, 220, 0)));
        SetBusy(true);
        Check(!UndoButton.IsEnabled && !RedoButton.IsEnabled, "History disabled while busy");
        SetBusy(false);
        Check(UndoButton.IsEnabled, "History restored after busy");
        await ApplyHistoryAsync(true);
        Check(!UndoButton.IsEnabled && RedoButton.IsEnabled && (await _facade.HasUnsavedChangesAsync(session.SessionId)).Value,
            "Full Undo remains conservatively dirty");
        var creating = CreateNewDocumentAsync();
        await ClickDecision("CloseButton");
        await creating;
        Check(_session!.SessionId == session.SessionId && RedoButton.IsEnabled && !_isBusy, "Full Undo Cancel preserves history");
        creating = CreateNewDocumentAsync();
        await ClickDecision("SecondaryButton");
        await creating;
        Check(_session!.SessionId != session.SessionId && _session.PageCount == 1
            && !(await _facade.HasUnsavedChangesAsync(_session.SessionId)).Value, "Explicit Discard creates fresh clean A4");
    }
}
