using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in harness uses real WinUI navigation and the native document/history.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing output directory.");
        var log = Path.Combine(output, "page-navigation-smoke.log");
        UnhandledException += (_, error) => File.AppendAllText(log, "\nUNHANDLED " + error.Exception);
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.PageNavigationSmokeAsync();
            File.AppendAllText(log, "PASS validation; cancel; first/last page; current zoom; hidden panels; unchanged native history; stale order/session; busy/organize/modal guards; Ctrl+G wiring.");
        }
        catch (Exception error) { File.AppendAllText(log, "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task PageNavigationSmokeAsync()
    {
        await WaitForNavigationAsync(() => Content.XamlRoot is not null);
        if (GoToPageButton.IsEnabled) throw new Exception("Go enabled without a document.");
        await GoToPageAsync();
        await OpenDocumentAsync("Page navigation smoke", await File.ReadAllBytesAsync(SamplePath));
        await WaitForNavigationAsync(() => PageScroller.ViewportHeight > 0 && GoToPageButton.IsEnabled);
        if (_session!.PageCount < 2) throw new Exception("Smoke needs multiple pages.");
        var shortcut = GoToPageButton.KeyboardAccelerators.Single();
        if (shortcut.Key != global::Windows.System.VirtualKey.G
            || shortcut.Modifiers != global::Windows.System.VirtualKeyModifiers.Control)
            throw new Exception("Ctrl+G wiring changed.");

        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0,
            new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        await ApplyHistoryAsync(true);
        var before = (await _facade.AnnotationStateAsync(_session.SessionId)).Value!;
        var navigating = GoToPageAsync();
        var dialog = await WaitForNavigationDialogAsync();
        var number = (NumberBox)dialog.Content;
        if (number.Value != _firstVisiblePage + 1) throw new Exception("Initial page differs from viewport.");
        foreach (var invalid in new[] { double.NaN, 0, -1, 1.5, _session.PageCount + 1.0 })
        {
            number.Value = invalid;
            if (dialog.IsPrimaryButtonEnabled) throw new Exception("Invalid page enabled Go.");
        }
        number.Value = _session.PageCount;
        if (!dialog.IsPrimaryButtonEnabled) throw new Exception("Last page disabled Go.");
        var offset = PageScroller.VerticalOffset;
        dialog.Hide();
        await navigating;
        if (PageScroller.VerticalOffset != offset || _dialogOpen) throw new Exception("Cancel navigated or kept modal open.");

        PanelsButton.IsChecked = false;
        PanelsButton_Click(PanelsButton, new RoutedEventArgs());
        navigating = GoToPageAsync();
        dialog = await WaitForNavigationDialogAsync(dialog);
        ((NumberBox)dialog.Content).Value = _session.PageCount;
        ZoomInButton_Click(ZoomInButton, new RoutedEventArgs());
        PageStack.UpdateLayout();
        InvokeGo(dialog);
        await navigating;
        await WaitForNavigationAsync(() => _firstVisiblePage == _slots.Count - 1);
        if (PageNavigationList.SelectedIndex != _firstVisiblePage || PanelsButton.IsChecked != false)
            throw new Exception("Page list or panels changed unexpectedly.");
        navigating = GoToPageAsync();
        dialog = await WaitForNavigationDialogAsync(dialog);
        ((NumberBox)dialog.Content).Value = 1;
        InvokeGo(dialog);
        await navigating;
        await WaitForNavigationAsync(() => _firstVisiblePage == 0 && PageScroller.VerticalOffset == 0);
        var after = (await _facade.AnnotationStateAsync(_session.SessionId)).Value!;
        if (before.CanUndo != after.CanUndo || before.CanRedo != after.CanRedo
            || !before.Annotations.SequenceEqual(after.Annotations)) throw new Exception("Navigation changed native history.");

        foreach (var guard in new[] { "busy", "organize", "order", "session" })
        {
            navigating = GoToPageAsync();
            dialog = await WaitForNavigationDialogAsync(dialog);
            ((NumberBox)dialog.Content).Value = _session.PageCount;
            if (guard == "order")
            {
                var moved = await _facade.EditPagesAsync(_session.SessionId, new PageEdit.Move(0, _session.PageCount - 1));
                if (!moved.IsSuccess) throw new Exception(moved.Error!.Message);
                _session = moved.Value!;
                BuildPagePlaceholders(_session);
            }
            if (guard == "session") await OpenDocumentAsync("Replacement", await File.ReadAllBytesAsync(SamplePath));
            _isBusy = guard == "busy";
            _organizing = guard == "organize";
            offset = PageScroller.VerticalOffset;
            InvokeGo(dialog);
            await navigating;
            if (PageScroller.VerticalOffset != offset) throw new Exception("Guard allowed stale navigation.");
            _isBusy = _organizing = false;
            if (guard == "order")
            {
                var saved = await _facade.SaveToDestinationAsync(_session.SessionId, _ => Task.CompletedTask);
                if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
                await RefreshAnnotationStateAsync();
            }
            UpdatePageNavigationControls();
        }
        foreach (var guard in new[] { "busy", "organize", "modal" })
        {
            _isBusy = guard == "busy";
            _organizing = guard == "organize";
            _dialogOpen = guard == "modal";
            UpdatePageNavigationControls();
            if (guard != "modal" && GoToPageButton.IsEnabled) throw new Exception("Guard left Go enabled.");
            await GoToPageAsync();
        }
        _isBusy = _organizing = _dialogOpen = false;
        UpdatePageNavigationControls();
        if (!GoToPageButton.IsEnabled) throw new Exception("Go did not recover.");
    }

    private async Task<ContentDialog> WaitForNavigationDialogAsync(ContentDialog? previous = null)
    {
        ContentDialog? dialog = null;
        await WaitForNavigationAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .SelectMany(popup => NavigationDescendants(popup.Child)).OfType<ContentDialog>()
                .FirstOrDefault(item => item != previous && item.Title?.ToString() == "Go to page");
            return dialog is not null;
        });
        return dialog!;
    }

    private static IEnumerable<DependencyObject> NavigationDescendants(DependencyObject root)
    {
        yield return root;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            foreach (var child in NavigationDescendants(VisualTreeHelper.GetChild(root, i))) yield return child;
    }

    private static void InvokeGo(ContentDialog dialog)
    {
        var button = NavigationDescendants(dialog).OfType<Button>().Single(item => item.Content?.ToString() == "Go");
        ((IInvokeProvider)new ButtonAutomationPeer(button).GetPattern(PatternInterface.Invoke)).Invoke();
    }

    private static async Task WaitForNavigationAsync(Func<bool> ready)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!ready())
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("WinUI did not reach the expected state.");
            await Task.Delay(50);
        }
    }
}
