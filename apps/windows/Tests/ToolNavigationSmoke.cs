using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Facade;

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
            File.WriteAllText(log, "PASS tools-tab clicks/reclicks synchronize rail; Edit content preserves selection; Home/Recent return; document replacement; Organize return/toggle; rail navigation; Comments clears unrelated rail selection; hidden views preserve destination; unchanged session/history.");
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
        static void Toggle(ToggleButton button) =>
            ((IToggleProvider)new ToggleButtonAutomationPeer(button).GetPattern(PatternInterface.Toggle)).Toggle();
        void CheckRail(string destination)
        {
            Check(_railDestination == destination, $"Rail destination must be {destination}, not {_railDestination}.");
            foreach (var button in AppRail.Children.OfType<ToggleButton>())
                Check(button.IsChecked == ((string)button.Tag == destination), $"Wrong rail highlight: {button.Tag} for {destination}.");
        }
        void CheckTool(string key)
        {
            Check(_selectedToolPage == key, "Selected tools-page owner disagrees.");
            foreach (var (name, page) in _toolPages)
                Check(page.Visibility == (name == key ? Visibility.Visible : Visibility.Collapsed), $"Wrong tools page: {name}.");
            foreach (var tab in _toolsTabStrip.Children.OfType<ToggleButton>())
                Check(tab.IsChecked == ((string)tab.Tag == key), $"Wrong tools tab: {tab.Tag}.");
            CheckRail(key);
        }

        await Wait(() => _shellInitialized && Content.XamlRoot is not null, "Window not loaded.");
        var sample = await File.ReadAllBytesAsync(SamplePath);
        await OpenDocumentAsync(SampleDisplayName, sample);
        await RefreshAnnotationStateAsync();
        CheckTool("Annotate");
        var session = _session!.SessionId;

        foreach (var key in new[] { "Edit", "Annotate", "Sign", "Comments", "Edit" })
        {
            var tab = _toolsTabStrip.Children.OfType<ToggleButton>().Single(t => (string)t.Tag == key);
            Toggle(tab);
            CheckTool(key);
            Toggle(tab);
            CheckTool(key);
        }
        Toggle(ContentEditButton);
        Check(_contentEditMode && ContentEditButton.IsChecked == true, "Content editing did not arm.");
        CheckTool("Edit");
        foreach (var destination in new[] { "Home", "Recent" })
        {
            ShowHomeView(destination);
            CheckRail(destination);
            ReturnToDocument_Click(ReturnToDocumentButton, new RoutedEventArgs());
            CheckTool("Edit");
            Check(_contentEditMode, "Home return changed the editing mode.");
        }
        Check(_session.SessionId == session && !UndoButton.IsEnabled && !RedoButton.IsEnabled,
            "Navigation changed the document or history.");

        ShowHomeView("Home");
        await OpenDocumentAsync("Replacement sample", sample);
        await RefreshAnnotationStateAsync();
        CheckTool("Edit");
        Check(!_contentEditMode, "A replacement document retained the previous content-edit mode.");

        await EnterOrganizeViewAsync();
        Check(_organizing, "Organize did not open.");
        CheckRail("Organize");
        SelectToolPage("Sign");
        CheckRail("Organize");
        SelectToolPage("Edit");
        OrganizeReturnButton_Click(OrganizeReturnButton, new RoutedEventArgs());
        CheckTool("Edit");
        await EnterOrganizeViewAsync();
        OrganizeButton_Click(OrganizeButton, new RoutedEventArgs());
        CheckTool("Edit");

        await EnterOrganizeViewAsync();
        await NavigateToToolAsync("Sign", new RoutedEventArgs());
        CheckTool("Sign");
        await NavigateToToolAsync("Edit", new RoutedEventArgs());
        CheckTool("Edit");
        Check(_contentEditMode, "Edit rail navigation did not arm content editing.");
        foreach (var destination in new[] { "Home", "Recent", "Files" })
        {
            ShowHomeView(destination);
            SelectToolPage("Comments");
            CheckRail(destination);
            ReturnToDocument_Click(ReturnToDocumentButton, new RoutedEventArgs());
            CheckTool("Comments");
        }
        Check(!UndoButton.IsEnabled && !RedoButton.IsEnabled, "Navigation changed native history.");
    }
}
