using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in harness drives the actual text-position dialog through the native facade.
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
            await _window.TextMoveSmokeAsync();
            File.WriteAllText(Path.Combine(output, "text-move-smoke.log"),
                "PASS native Move text dialog; repeated moves and preview; stale-target refusal; cancel/no-op history; one Undo/Redo; final position save/reopen; click-insert then move/retype/move with one insertion Undo/Redo and final save/reopen.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "text-move-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task TextMoveSmokeAsync()
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenDocumentAsync("Text move smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        var sessionId = _session.SessionId;
        var original = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![0];
        await DriveTextMoveAsync(80, 90);
        await DriveTextMoveAsync(120, 140);
        var current = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![0];
        if (current.Id != original.Id || current.Text != original.Text
            || current.Bounds.X != 120 || current.Bounds.Y != 140 || !_annotationState!.CanUndo
            || _isBusy || _editingTextGeometry || _dialogOpen)
            throw new Exception("Repeated movement lost target, coordinates or control state.");
        var stale = await _facade.MoveTextRunAsync(sessionId, original, 180, 190);
        if (stale.IsSuccess) throw new Exception("A stale revision-bound target was accepted.");
        await DriveTextMoveAsync(120, 140); // no-op
        await DriveTextMoveAsync(180, 190, cancel: true);
        var unchanged = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![0];
        if (unchanged.Bounds != current.Bounds) throw new Exception("Cancel/no-op changed position.");
        await ApplyHistoryAsync(true);
        var restored = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![0];
        if (restored.Bounds != original.Bounds || _annotationState!.CanUndo || !_annotationState.CanRedo)
            throw new Exception("Repeated moves did not share one Undo step.");
        await ApplyHistoryAsync(false);
        var redone = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![0];
        if (redone.Bounds != current.Bounds) throw new Exception("Redo lost the final destination.");
        var saved = await _facade.SaveToDestinationAsync(sessionId, _ => Task.CompletedTask);
        if (!saved.IsSuccess) throw new Exception("Amended movement did not save: " + saved.Error?.Message);
        var core = new GeneratedPdfCore();
        using var reopened = core.OpenFromBytes(saved.Value!.Bytes, null);
        var written = core.ReadPageContent(reopened, 0).TextRuns.Single(run => run.Text == original.Text);
        if (Math.Abs(written.Bbox.X - 120) > 0.01 || Math.Abs(written.Bbox.Y - 140) > 0.01)
            throw new Exception("Saved PDF did not preserve final coordinates.");

        // Start with clean history so one Undo must remove the whole insertion.
        await ApplyHistoryAsync(true);
        SetContentInsertMode(ContentInsertKind.Text);
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(72, 40), null!);
        if (_insertEditor is null) throw new Exception("Canvas insertion did not open.");
        const string insertedText = "Text move smoke insertion";
        _insertEditor.Box.Text = insertedText;
        await CommitInsertEditorAsync();
        SetContentInsertMode(null);
        var targets = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value!;
        var index = targets.Select((run, i) => (run, i)).Single(pair => pair.run.Text == insertedText).i;
        var inserted = targets[index];
        await DriveTextMoveAsync(80, 90, selectedIndex: index);
        var moved = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![index];
        await BeginContentGestureAsync(_slots[0], 0,
            new AnnotationPoint(moved.Bounds.X + 2, moved.Bounds.Y + moved.Bounds.Height / 2), null!);
        if (_pump.Box is null || !_pump.Box.Run.IsPendingInsertion)
            throw new Exception("Moved insertion did not reopen through canvas hit-testing.");
        const string finalText = "Text move smoke retyped insertion";
        _pump.Box.Box.Text = finalText;
        await CommitContentEditorAsync();
        await DriveTextMoveAsync(120, 140, selectedIndex: index);
        var finalInsertion = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![index];
        if (finalInsertion.Id != inserted.Id || finalInsertion.Text != finalText
            || finalInsertion.Bounds.X != 120 || finalInsertion.Bounds.Y != 140
            || finalInsertion.Bounds.Height != inserted.Bounds.Height)
            throw new Exception("Insertion movement lost identity, text or geometry.");
        await ApplyHistoryAsync(true);
        var removed = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value!;
        if (removed.Any(run => run.Id == inserted.Id) || _annotationState!.CanUndo)
            throw new Exception("Insertion movement stacked history instead of amending.");
        await ApplyHistoryAsync(false);
        var restoredInsertion = (await _facade.PageTextEditTargetsAsync(sessionId, 0)).Value![index];
        if (restoredInsertion.Text != finalText || restoredInsertion.Bounds != finalInsertion.Bounds)
            throw new Exception("Insertion Redo lost final text or position.");
        var insertedSave = await _facade.SaveToDestinationAsync(sessionId, _ => Task.CompletedTask);
        if (!insertedSave.IsSuccess) throw new Exception(insertedSave.Error!.Message);
        using var reopenedInsertion = core.OpenFromBytes(insertedSave.Value!.Bytes, null);
        var writtenInsertion = core.ReadPageContent(reopenedInsertion, 0).TextRuns.Single(run => run.Text == finalText);
        if (Math.Abs(writtenInsertion.Bbox.X - 120) > 0.01 || Math.Abs(writtenInsertion.Bbox.Y - 140) > 0.01)
            throw new Exception("Saved insertion lost final coordinates.");
    }

    private async Task DriveTextMoveAsync(double x, double y, bool cancel = false, int selectedIndex = 0)
    {
        var editing = MoveTextGeometryAsync();
        ContentDialog? dialog = null;
        for (var attempt = 0; dialog is null && attempt < 100; attempt++)
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .SelectMany(popup => TextMoveDescendants(popup.Child)).OfType<ContentDialog>()
                .FirstOrDefault(item => item.Title?.ToString()?.StartsWith("Move text") == true);
            if (dialog is null) await Task.Delay(50);
        }
        if (dialog is null) throw new Exception("Move text dialog did not open.");
        ((StackPanel)dialog.Content).Children.OfType<ComboBox>().Single().SelectedIndex = selectedIndex;
        var fields = ((StackPanel)dialog.Content).Children.OfType<NumberBox>().ToArray();
        fields[0].Value = x;
        fields[1].Value = y;
        if (cancel) dialog.Hide();
        else
        {
            if (!dialog.IsPrimaryButtonEnabled) throw new Exception("Valid text coordinates disabled Move.");
            var button = TextMoveDescendants(dialog).OfType<Button>().Single(item => item.Content?.ToString() == "Move");
            ((IInvokeProvider)new ButtonAutomationPeer(button).GetPattern(PatternInterface.Invoke)).Invoke();
        }
        await editing;
    }

    private static IEnumerable<DependencyObject> TextMoveDescendants(DependencyObject root)
    {
        yield return root;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            foreach (var child in TextMoveDescendants(VisualTreeHelper.GetChild(root, i))) yield return child;
    }
}
