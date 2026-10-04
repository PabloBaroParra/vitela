using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
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
            await _window.SearchSmokeAsync();
            File.WriteAllText(Path.Combine(output, "search-smoke.log"),
                "PASS native Find popup/entry focus/hint, no-document/empty/busy gates, real exact-case matches/first status/wrap navigation, preserved whitespace, no-result/clear/error and supersession, Tools stays folded and history unchanged.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "search-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task SearchSmokeAsync()
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
        await Wait(() => Content.XamlRoot is not null, "Window not loaded");
        UpdateSearchControls();
        Check(!_findDocumentButton.IsEnabled && !SearchButton.IsEnabled, "No-document Find gates");
        await RunSearchAsync();
        Check(SearchStatus.Text == "Open a PDF before searching.", "No-document status");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        var session = _session!;
        SetToolsPanelVisible(false);
        _documentFindFlyout.ShowAt(_findDocumentButton);
        await Wait(() => SearchBox.FocusState != FocusState.Unfocused, "Popup entry focus");
        Check(Descendants((DependencyObject)_documentFindFlyout.Content).OfType<TextBlock>()
            .Any(t => t.Text == "Case-sensitive. Press Enter to search."), "Exact hint");
        var targets = await _facade.PageTextEditTargetsAsync(session.SessionId, 0);
        Check(targets.IsSuccess && targets.Value!.Count > 0, "Sample text fixture");
        var query = targets.Value!.First(r => r.Text.Any(char.IsLetter)).Text.Split(' ', StringSplitOptions.RemoveEmptyEntries).First();
        SearchBox.Text = query;
        await RunSearchAsync();
        await Wait(() => _paintedHit is not null, "First match must be selected/painted");
        Check(SearchResultsList.Items.Count > 0 && SearchStatus.Text == Viewer.SearchSelection.Status(query, 0, SearchResultsList.Items.Count), "Exact match status");
        Check(_searchHitsByPage.Values.SelectMany(h => h).All(h => h.Text.Contains(query, StringComparison.Ordinal)), "Matcher case-sensitive contract");
        var count = SearchResultsList.Items.Count;
        StepMatch(-1);
        await Wait(() => SearchStatus.Text == Viewer.SearchSelection.Status(query, count - 1, count), "Previous wrap");
        StepMatch(1);
        await Wait(() => SearchStatus.Text == Viewer.SearchSelection.Status(query, 0, count), "Next wrap");
        Check(!_toolsColumn.IsOpen && SearchStatus.Visibility == Visibility.Visible, "Find must not force Tools open or hide status");
        SearchBox.Text = " " + query + " ";
        await RunSearchAsync();
        Check(SearchStatus.Text.Contains("\" " + query + " \""), "Search must preserve surrounding whitespace");
        var missing = "no-match-" + Guid.NewGuid().ToString("N");
        SearchBox.Text = query;
        var old = RunSearchAsync();
        SearchBox.Text = missing;
        var latest = RunSearchAsync();
        await Task.WhenAll(old, latest);
        Check(SearchStatus.Text == $"No matches for \"{missing}\"." && SearchResultsList.Items.Count == 0
            && !PreviousMatchButton.IsEnabled && !NextMatchButton.IsEnabled && !_searchPending, "Superseded search published stale matches");
        SearchBox.Text = "";
        await RunSearchAsync();
        Check(SearchStatus.Text == "Enter text to find." && _slots.All(s => s.SearchHighlights.Children.Count == 0), "Clear removes matches/highlights");
        SetBusy(true);
        Check(!SearchBox.IsEnabled && !_findDocumentButton.IsEnabled && !SearchButton.IsEnabled, "Busy Find gates");
        SetBusy(false);
        _session = session with { PageCount = 0, State = Facade.DocumentSessionState.Empty };
        UpdateSearchControls();
        Check(!_findDocumentButton.IsEnabled && !SearchButton.IsEnabled, "Empty-page gates");
        _session = session with { SessionId = "stale" };
        SearchBox.Text = query;
        await RunSearchAsync();
        Check(SearchStatus.Text.StartsWith("Could not search:") && SearchResultsList.Items.Count == 0 && !_searchPending, "Error must leave empty results and restore state");
        _session = session;
        ClearSearchResults();
        _documentFindFlyout.Hide();
        Check(!UndoButton.IsEnabled && !RedoButton.IsEnabled && _session.SessionId == session.SessionId, "Search changed history/session");
    }
}
