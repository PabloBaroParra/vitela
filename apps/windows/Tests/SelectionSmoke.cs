using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Windows.ApplicationModel.DataTransfer;

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
            await _window.SelectionSmokeAsync();
            File.WriteAllText(Path.Combine(output, "selection-smoke.log"),
                "PASS real character selection/line highlights/text and clipboard output; same-session invalidation discards an in-flight character handle and content parse; session recheck clears selection without replacing clipboard; empty-copy guidance; read-only edit independence; content mode disarms selection; mode reentry refuses obsolete inline editor requests; history unchanged.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "selection-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task SelectionSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        for (var i = 0; i < 200 && Content.XamlRoot is null; i++) await Task.Delay(25);
        Check(Content.XamlRoot is not null, "Window not loaded");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        var session = _session!;
        Check(CanSelectDocumentText(), "Sample must permit extraction");
        var load = EnsurePageCharactersAsync(0);
        var old = _pageText[0];
        InvalidatePageCharacters(0);
        await load;
        Check(old.Handle is null && !_pageText.ContainsKey(0), "Invalidated load retained its handle");
        await EnsurePageCharactersAsync(0);
        Check(_pageText[0].Handle is not null, "Fresh character load failed");
        var target = (await _facade.PageTextEditTargetsAsync(session.SessionId, 0)).Value!.First(r => r.Text.Length > 2);
        var rect = target.Bounds;
        _textSelection = new TextDrag(0, new(rect.X, rect.Y + rect.Height / 2), new(rect.X + rect.Width, rect.Y + rect.Height / 2));
        RedrawSelection();
        Check(SelectedText() is { Length: > 0 } && _slots[0].Selection.Children.Count > 0, "Selection has no text/highlights");
        var selected = SelectedText();
        CopySelectedDocumentText();
        Check(AnnotationStatus.Text == "Text copied." && await Clipboard.GetContent().GetTextAsync() == selected,
            "Clipboard does not contain the selected text");
        var contentState = new PageContentState();
        // The session remains current, but this entry was retired by a page edit.
        var content = await LoadPageContentAsync(session.SessionId, 0, contentState);
        Check(content is null && contentState.Content is null, "Orphan content parse was published");
        _session = session with { ContentEditingAllowed = false };
        Check(CanSelectDocumentText(), "Content edit permission must not own extraction");
        _session = session with { SessionId = "retired-selection" };
        Check(!CanSelectDocumentText() && _textSelection is null && !_textDragActive
            && _slots.All(s => s.Selection.Children.Count == 0), "Stale selection was not cleared");
        CopySelectedDocumentText();
        Check(await Clipboard.GetContent().GetTextAsync() == selected, "Refused copy overwrote clipboard");
        _session = session;
        CopySelectedDocumentText();
        Check(AnnotationStatus.Text == "Select text on a page before copying.", "Empty copy omitted guidance");
        _textSelection = new TextDrag(0, new(rect.X, rect.Y), new(rect.X + rect.Width, rect.Y));
        SetContentEditMode(true);
        Check(_textSelection is null && !_textDragActive, "Content mode retained text selection");
        _pageContent.Remove(0);
        var obsolete = OpenContentEditorAsync(0, new(rect.X + 2, rect.Y + rect.Height / 2));
        SetContentEditMode(false);
        SetContentEditMode(true);
        await obsolete;
        Check(_pump.Box is null, "Obsolete click reopened an editor after mode reentry");
        SetContentEditMode(false);
        Check(!UndoButton.IsEnabled && !RedoButton.IsEnabled, "Selection modified history");
    }
}
