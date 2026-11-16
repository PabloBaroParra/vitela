using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

// Opt-in native navigation regression. Never opens or edits a user document.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        var log = Path.Combine(output, "tool-navigation-smoke.log");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.ToolNavigationSmokeAsync();
            File.WriteAllText(log, "PASS mode-bar clicks/reclicks follow the tools page; Edit arms content editing; Comments is a section of Annotate; Home returns; document replacement; Organize return/toggle; mode-bar navigation from Organize; the guard refuses a click and resyncs the tab; Protect is never checked; unchanged session/history.");
        }
        catch (Exception error) { File.WriteAllText(log, "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ToolNavigationSmokeAsync()
    {
        static void Check(bool value, string message)
        {
            if (!value) throw new InvalidOperationException(message);
        }
        async Task Wait(Func<bool> predicate, string message)
        {
            for (var i = 0; i < 200 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        ButtonBase Tab(ShellMode mode) => _modeButtons.Single(entry => entry.Mode == mode).Button;
        // A real UI Automation click, as a screen reader or test-*.ps1 sends it.
        void Click(ShellMode mode)
        {
            if (Tab(mode) is ToggleButton toggle)
                ((IToggleProvider)new ToggleButtonAutomationPeer(toggle).GetPattern(PatternInterface.Toggle)).Toggle();
            else
                ((IInvokeProvider)new ButtonAutomationPeer((Button)Tab(mode)).GetPattern(PatternInterface.Invoke)).Invoke();
        }
        bool ShowsMode(ShellMode expected) =>
            _modeButtons.All(entry => entry.Button is not ToggleButton toggle || toggle.IsChecked == (entry.Mode == expected));
        void CheckMode(ShellMode expected)
        {
            foreach (var (mode, button) in _modeButtons)
                if (button is ToggleButton toggle)
                    Check(toggle.IsChecked == (mode == expected), $"Wrong mode-bar tab: {mode} for {expected}.");
        }
        bool OnTool(string key) => _selectedToolPage == key && EditorView.Visibility == Visibility.Visible;
        void CheckTool(string key)
        {
            Check(_selectedToolPage == key, $"Selected tools-page owner disagrees: {_selectedToolPage}, not {key}.");
            foreach (var (name, page) in _toolPages)
                Check(page.Visibility == (name == key ? Visibility.Visible : Visibility.Collapsed), $"Wrong tools page: {name}.");
            CheckMode(ShellModes.Current(ShellScreen.Editor, key)!.Value);
        }

        await Wait(() => _shellInitialized && Content.XamlRoot is not null, "Window not loaded.");
        CheckMode(ShellMode.Home);
        Check(Tab(ShellMode.Protect) is Button, "Protect must be a plain button, never a place.");
        Check(_toolPages.Keys.Order().SequenceEqual(new[] { "Annotate", "Edit", "Sign" }),
            "The tools panel must have exactly the Edit, Annotate and Sign pages.");
        Check(_toolPages[ShellModes.AnnotatePage].Children.Contains(_commentsList), "Comments must be a section of the Annotate page.");

        var sample = await File.ReadAllBytesAsync(SamplePath);
        await OpenDocumentAsync(SampleDisplayName, sample);
        await RefreshAnnotationStateAsync();
        CheckTool("Annotate");
        var session = _session!.SessionId;

        foreach (var (mode, key) in new[] { (ShellMode.Edit, "Edit"), (ShellMode.Annotate, "Annotate"), (ShellMode.Sign, "Sign"), (ShellMode.Edit, "Edit") })
        {
            // Twice: clicking the current mode must never leave every tab unchecked.
            for (var click = 0; click < 2; click++)
            {
                Click(mode);
                await Wait(() => OnTool(key) && ShowsMode(mode), $"The {mode} tab did not land on its page.");
                CheckTool(key);
            }
        }
        Check(_contentEditMode && ContentEditButton.IsChecked == true, "The Edit mode did not arm content editing.");

        Click(ShellMode.Home);
        await Wait(() => HomeView.Visibility == Visibility.Visible, "The Home tab did not show Home.");
        CheckMode(ShellMode.Home);
        // A page chosen while Home is up moves no tab: the bar shows the screen, not the panel.
        SelectToolPage("Sign");
        CheckMode(ShellMode.Home);
        ReturnToDocument_Click(ReturnToDocumentButton, new RoutedEventArgs());
        CheckTool("Sign");
        SelectToolPage("Edit");
        CheckTool("Edit");
        Check(_contentEditMode, "Home return changed the editing mode.");
        Check(_session.SessionId == session && !UndoButton.IsEnabled && !RedoButton.IsEnabled,
            "Navigation changed the document or history.");

        // The bar's guard: a click while a dialog is up runs nothing, and the toggle it flipped is put back.
        _dialogOpen = true;
        try
        {
            Click(ShellMode.Home);
            await Task.Delay(100);
            Check(HomeView.Visibility == Visibility.Collapsed, "A guarded mode click switched the screen.");
            CheckTool("Edit");
        }
        finally { _dialogOpen = false; }

        ShowHomeView();
        await OpenDocumentAsync("Replacement sample", sample);
        await RefreshAnnotationStateAsync();
        CheckTool("Edit");
        Check(!_contentEditMode, "A replacement document retained the previous content-edit mode.");

        await EnterOrganizeViewAsync();
        Check(_organizing, "Organize did not open.");
        CheckMode(ShellMode.Organize);
        SelectToolPage("Sign");
        CheckMode(ShellMode.Organize);
        SelectToolPage("Edit");
        OrganizeReturnButton_Click(OrganizeReturnButton, new RoutedEventArgs());
        CheckTool("Edit");
        await EnterOrganizeViewAsync();
        OrganizeButton_Click(OrganizeButton, new RoutedEventArgs());
        CheckTool("Edit");

        Click(ShellMode.Organize);
        await Wait(() => _organizing && ShowsMode(ShellMode.Organize), "The Organize tab did not open Organize.");
        Click(ShellMode.Sign);
        await Wait(() => !_organizing && OnTool("Sign"), "The Sign tab did not leave Organize for Fill & Sign.");
        CheckTool("Sign");
        Click(ShellMode.Edit);
        await Wait(() => OnTool("Edit") && _contentEditMode, "The Edit tab did not leave for Edit with content editing armed.");
        CheckTool("Edit");

        // A screen changed by another door still shows as its mode.
        await RunPaletteCommandAsync(PaletteCommand.ModeAnnotate);
        CheckTool("Annotate");
        await RunPaletteCommandAsync(PaletteCommand.ModeHome);
        CheckMode(ShellMode.Home);
        ReturnToDocument_Click(ReturnToDocumentButton, new RoutedEventArgs());
        CheckTool("Annotate");
        Check(!UndoButton.IsEnabled && !RedoButton.IsEnabled, "Navigation changed native history.");
    }
}
