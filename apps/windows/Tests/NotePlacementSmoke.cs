using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Automation.Provider;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in WinUI runtime harness; compiled instead of App.xaml.cs via the targets file.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing output directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.NotePlacementSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "note-smoke.log"), "PASS blank validation; cancel without history; multiline placement; undo/redo; saved PDF; read pending/saved/empty/restricted session notes without history; reading guards; stale session; forbidden/busy/organize guards.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "note-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task NotePlacementSmokeAsync(string output)
    {
        await WaitForAsync(() => Content.XamlRoot is not null);
        await OpenDocumentAsync("Note smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        var before = _annotationState!;
        var drag = new PointerDrag(0, new AnnotationPoint(40, 80), null, AnnotationKind.TextNote, null)
        {
            Current = new AnnotationPoint(140, 120),
        };
        var placement = CommitPlacementAsync(0, AnnotationKind.TextNote, drag);
        var dialog = await WaitForNoteDialogAsync();
        var text = (TextBox)dialog.Content;
        if (dialog.IsPrimaryButtonEnabled || !text.AcceptsReturn) throw new Exception("Empty note enabled or multiline input unavailable.");
        text.Text = " \r\n\t ";
        await Task.Delay(100);
        if (dialog.IsPrimaryButtonEnabled) throw new Exception("Whitespace note enabled.");
        text.Text = "Discard me";
        dialog.Hide();
        await placement;
        var canceled = (await _facade.AnnotationStateAsync(_session.SessionId)).Value!;
        if (canceled.Annotations.Count != before.Annotations.Count || canceled.CanUndo != before.CanUndo || canceled.CanRedo != before.CanRedo)
            throw new Exception("Cancel changed annotation state or history.");

        placement = CommitPlacementAsync(0, AnnotationKind.TextNote, drag);
        dialog = await WaitForNoteDialogAsync(dialog);
        text = (TextBox)dialog.Content;
        text.Text = "  First line\rSecond line  ";
        await WaitForAsync(() => dialog.IsPrimaryButtonEnabled);
        InvokeAdd(dialog);
        await placement;
        if (_annotationState!.Annotations.Count != before.Annotations.Count + 1 || !_annotationState.CanUndo)
            throw new Exception("Add did not record one annotation.");
        var note = _annotationState.Annotations[^1];
        if (note.Kind != AnnotationKind.TextNote || note.Rect != new AnnotationRect(40, 80, 100, 40))
            throw new Exception("Placement lost note kind or traced geometry.");
        if (note.Contents != text.Text) throw new Exception("Native snapshot lost pending note contents.");
        await CheckNoteReadingAsync(note, text.Text);
        await ApplyHistoryAsync(true);
        if (_annotationState!.Annotations.Count != before.Annotations.Count || _annotationState.CanUndo != before.CanUndo)
            throw new Exception("One undo did not restore the initial state.");
        await ApplyHistoryAsync(false);
        if (_annotationState!.Annotations.Count != before.Annotations.Count + 1 || _isBusy || !NoteButton.IsEnabled)
            throw new Exception("Redo or control restoration failed.");
        var saved = await _facade.SaveToDestinationAsync(_session.SessionId, bytes =>
            File.WriteAllBytesAsync(Path.Combine(output, "note-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);

        await RefreshAnnotationStateAsync();
        note = _annotationState!.Annotations.Single(annotation => annotation.Kind == AnnotationKind.TextNote && annotation.Contents == "  First line\rSecond line  ");
        await CheckNoteReadingAsync(note, note.Contents!);
        _annotationState = _annotationState with { EditingAllowed = false };
        await CheckNoteReadingAsync(note, note.Contents!);
        _annotationState = _annotationState with
        {
            Annotations = _annotationState.Annotations.Select(annotation => annotation.Id == note.Id ? annotation with { Contents = string.Empty } : annotation).ToArray(),
        };
        await CheckNoteReadingAsync(note, string.Empty);
        _selectedAnnotationId = null;
        UpdateAnnotationControls(_annotationState);
        if (ReadNoteButton.IsEnabled) throw new Exception("Read enabled without selection.");
        await ReadSelectedNoteAsync();
        _selectedAnnotationId = note.Id;
        _isBusy = true;
        UpdateAnnotationControls(_annotationState);
        if (ReadNoteButton.IsEnabled) throw new Exception("Read enabled while busy.");
        await ReadSelectedNoteAsync();
        _isBusy = false;
        _organizing = true;
        UpdateAnnotationControls(_annotationState);
        if (ReadNoteButton.IsEnabled) throw new Exception("Read enabled while organizing.");
        await ReadSelectedNoteAsync();
        _organizing = false;
        var fixtureSave = await _facade.SaveToDestinationAsync(_session!.SessionId,
            bytes => File.WriteAllBytesAsync(Path.Combine(Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")!, "note-history-fixture.pdf"), bytes));
        if (!fixtureSave.IsSuccess) throw new Exception(fixtureSave.Error!.Message);
        await OpenDocumentAsync("Placement guards", await File.ReadAllBytesAsync(SamplePath));

        placement = CommitPlacementAsync(0, AnnotationKind.TextNote, drag);
        dialog = await WaitForNoteDialogAsync(dialog);
        ((TextBox)dialog.Content).Text = "Wrong session";
        await WaitForAsync(() => dialog.IsPrimaryButtonEnabled);
        await OpenDocumentAsync("Replacement", await File.ReadAllBytesAsync(SamplePath));
        var replacementCount = _annotationState!.Annotations.Count;
        InvokeAdd(dialog);
        await placement;
        if (_annotationState!.Annotations.Count != replacementCount || _annotationState.CanUndo)
            throw new Exception("Stale prompt edited the replacement session.");
        _annotationState = _annotationState with { EditingAllowed = false };
        await PlaceTextNoteAsync(0, new PdfCoreRect(40, 80, 100, 40));
        _annotationState = _annotationState with { EditingAllowed = true };
        _isBusy = true;
        await PlaceTextNoteAsync(0, new PdfCoreRect(40, 80, 100, 40));
        _isBusy = false;
        _organizing = true;
        await PlaceTextNoteAsync(0, new PdfCoreRect(40, 80, 100, 40));
        _organizing = false;
        if (_dialogOpen || _annotationState.CanUndo) throw new Exception("Guard opened a prompt or edited history.");
    }

    private async Task CheckNoteReadingAsync(Annotation note, string expected)
    {
        _selectedAnnotationId = note.Id;
        UpdateAnnotationControls(_annotationState);
        if (!ReadNoteButton.IsEnabled) throw new Exception("Selected note cannot be read.");
        var before = _annotationState!;
        var reading = ReadSelectedNoteAsync();
        var dialog = await WaitForNoteDialogAsync(title: $"Note — page {note.PageIndex + 1}");
        var text = (TextBox)dialog.Content;
        if (!text.IsReadOnly || !text.AcceptsReturn || text.Text != expected || dialog.Title?.ToString() != $"Note — page {note.PageIndex + 1}")
            throw new Exception($"Reading mismatch: readOnly={text.IsReadOnly}, multiline={text.AcceptsReturn}, title={dialog.Title}, actual=[{string.Join(',', text.Text.Select(c => (int)c))}], expected=[{string.Join(',', expected.Select(c => (int)c))}].");
        await ReadSelectedNoteAsync(); // A second request must not open a second modal.
        dialog.Hide();
        await reading;
        await WaitForAsync(() => !VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
            .Any(popup => FindNoteDialog(popup.Child) == dialog));
        var after = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        if (_dialogOpen || after.CanUndo != before.CanUndo || after.CanRedo != before.CanRedo || after.Annotations.Count != before.Annotations.Count)
            throw new Exception("Reading changed history or left a modal open.");
    }

    private async Task<ContentDialog> WaitForNoteDialogAsync(ContentDialog? previous = null, string? title = null)
    {
        ContentDialog? dialog = null;
        await WaitForAsync(() =>
        {
            dialog = VisualTreeHelper.GetOpenPopupsForXamlRoot(Content.XamlRoot)
                .Select(popup => FindNoteDialog(popup.Child)).FirstOrDefault(item => item is not null && item != previous && (title is null || item.Title?.ToString() == title));
            return dialog is not null;
        });
        return dialog!;
    }

    private static ContentDialog? FindNoteDialog(DependencyObject root)
    {
        if (root is ContentDialog dialog) return dialog;
        for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++)
            if (FindNoteDialog(VisualTreeHelper.GetChild(root, i)) is { } found) return found;
        return null;
    }

    private static void InvokeAdd(ContentDialog dialog)
    {
        var buttons = new List<Button>();
        void collect(DependencyObject root)
        {
            if (root is Button button) buttons.Add(button);
            for (var i = 0; i < VisualTreeHelper.GetChildrenCount(root); i++) collect(VisualTreeHelper.GetChild(root, i));
        }
        collect(dialog);
        var add = buttons.Single(button => button.Content?.ToString() == "Add");
        ((IInvokeProvider)new ButtonAutomationPeer(add).GetPattern(PatternInterface.Invoke)).Invoke();
    }

    private static async Task WaitForAsync(Func<bool> ready)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (!ready())
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("WinUI did not reach the expected state.");
            await Task.Delay(50);
        }
    }
}
