using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness, using the same replacement-entry-point pattern as the other smoke tests.
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
            await _window.ContentDeleteSmokeAsync();
            File.WriteAllText(Path.Combine(output, "content-delete-smoke.log"),
                "PASS no-target gate; target hint; click-focus/Tab contract; busy/permission gates; discard unrecorded keystrokes on canvas-target deletion; pending-retype deletion; one undo/redo; save/reopen.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "content-delete-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ContentDeleteSmokeAsync()
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenDocumentAsync("Delete smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        SetContentEditMode(true);
        if (DeleteTextButton.IsEnabled) throw new Exception("Delete enabled without a target.");
        var sessionId = _session.SessionId;
        var targets = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        var run = targets.Value?.FirstOrDefault(run => !run.RequiresFontSubstitution)
            ?? throw new Exception("No sample text target.");
        OpenEditorOver(0, run, run.Bounds.X);
        if (!DeleteTextButton.IsEnabled || _editTextHint.Text != "Text run selected — delete it, or retype it in place.")
            throw new Exception("Open editor did not publish its delete target.");
        if (DeleteTextButton.AllowFocusOnInteraction || !DeleteTextButton.IsTabStop)
            throw new Exception("Delete lost its click-focus/keyboard contract.");
        _isBusy = true;
        UpdateAnnotationControls(_annotationState);
        if (DeleteTextButton.IsEnabled) throw new Exception("Delete enabled while busy.");
        _isBusy = false;
        var session = _session;
        _session = session with { ContentEditingAllowed = false };
        UpdateAnnotationControls(_annotationState);
        if (DeleteTextButton.IsEnabled) throw new Exception("Delete enabled without content permission.");
        _session = session;
        UpdateAnnotationControls(_annotationState);
        _pump.Box!.Box.Text = run.Text + " not recorded";
        await DeleteOpenContentTextAsync();
        if (_pump.Box is not null || DeleteTextButton.IsEnabled || !_annotationState!.CanUndo)
            throw new Exception("Delete did not close its target and record undoable history.");
        var after = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!after.IsSuccess || after.Value!.Any(candidate => candidate.Id == run.Id))
            throw new Exception("Deleted run remains on the page.");
        await ApplyHistoryAsync(true);
        var restored = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!restored.IsSuccess || !restored.Value!.Any(candidate => candidate.Id == run.Id) || _annotationState!.CanUndo)
            throw new Exception("One undo did not restore the original run/history.");
        await ApplyHistoryAsync(false);
        after = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!after.IsSuccess || after.Value!.Any(candidate => candidate.Id == run.Id))
            throw new Exception("Redo did not remove the run again.");
        await ApplyHistoryAsync(true);
        restored = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        run = restored.Value!.Single(candidate => candidate.Id == run.Id);
        OpenEditorOver(0, run, run.Bounds.X);
        _pump.Box!.Box.Text = run.Text + " changed";
        _liveEdit.Stop();
        await _pump.PumpAsync();
        // Let the normal writer's queued selection refresh run: the harness
        // must not conceal missing post-write UI wiring by refreshing itself.
        var retyped = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!retyped.IsSuccess || retyped.Value!.Single(candidate => candidate.Id == run.Id).Text != run.Text + " changed")
            throw new Exception("Typing did not reach the real core before deletion.");
        if (!DeleteTextButton.IsEnabled || _editTextHint.Text != "Text run selected — delete it, or retype it in place.")
            throw new Exception("Pending retype lost its delete target/hint.");
        _pump.Box!.Box.Text = run.Text + " unrecorded after retype";
        await DeleteOpenContentTextAsync();
        if (_pump.Box is not null || DeleteTextButton.IsEnabled)
            throw new Exception("Pending-retype deletion did not close its target.");
        after = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!after.IsSuccess || after.Value!.Any(candidate => candidate.Id == run.Id))
            throw new Exception("Pending-retype deletion left its run on the page.");
        await ApplyHistoryAsync(true);
        restored = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!restored.IsSuccess || restored.Value!.Single(candidate => candidate.Id == run.Id).Text != run.Text || _annotationState!.CanUndo)
            throw new Exception("One undo did not restore the pre-retype original/history.");
        await ApplyHistoryAsync(false);
        after = await _facade.PageTextEditTargetsAsync(sessionId, 0);
        if (!after.IsSuccess || after.Value!.Any(candidate => candidate.Id == run.Id))
            throw new Exception("Redo did not restore the amended deletion.");
        var saved = await _facade.SaveToDestinationAsync(sessionId, _ => Task.CompletedTask);
        if (!saved.IsSuccess) throw new Exception("Amended deletion did not save: " + saved.Error?.Message);
        var check = new GeneratedPdfCore();
        using var reopened = check.OpenFromBytes(saved.Value!.Bytes, null);
        var savedRuns = check.ReadPageContent(reopened, 0).TextRuns;
        if (savedRuns.Any(candidate => candidate.Text == run.Text || candidate.Text == run.Text + " changed"))
            throw new Exception("Saved document retained the original or retyped run.");
    }
}
