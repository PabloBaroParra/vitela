using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Windows.Graphics.Imaging;
using Windows.Storage.Streams;

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
            var notes = await _window.CanvasInsertSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "canvas-insert-smoke.log"),
                "PASS insert toggles arm content editing and exclude each other; a miss click with Insert text opens a blank box at the click; "
                + "Escape records nothing; commit records one undoable run that saves at the click; image card Nothing/Ready/synthetic PendingEdit; "
                + "Delete acts on the canvas selection with one undo; leaving the mode disarms the insert kind. " + notes);
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "canvas-insert-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task<string> CanvasInsertSmokeAsync(string output)
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await ReopenForSmokeAsync(await File.ReadAllBytesAsync(SamplePath));

        // --- toggles ----------------------------------------------------------
        if (DeleteImageButton.IsEnabled || ReplaceImageButton.IsEnabled || _editImageHint.Text != NoImageSelectedHint)
            throw new Exception("Image commands enabled without a selection.");
        SetContentInsertMode(ContentInsertKind.Text);
        if (!_contentEditMode || ContentEditButton.IsChecked != true || InsertTextButton.IsChecked != true)
            throw new Exception("Insert text did not arm content editing.");
        SetContentInsertMode(ContentInsertKind.Image);
        if (InsertTextButton.IsChecked == true || InsertImageButton.IsChecked != true)
            throw new Exception("Insert kinds are not mutually exclusive.");
        SetContentInsertMode(ContentInsertKind.Text);

        // --- click-to-insert text ------------------------------------------------
        var point = new AnnotationPoint(72, 40);
        await BeginContentGestureAsync(_slots[0], 0, point, null!);
        if (_insertEditor is not { } box || box.Anchor != point || box.Target.PageIndex != 0)
            throw new Exception("A miss click did not open a blank box at the click.");
        if (!_slots[0].Content.Children.Contains(box.Box)) throw new Exception("Insert box is not on the page.");
        CancelInsertEditor();
        await Task.Delay(50);
        if (_insertEditor is not null || _annotationState!.CanUndo) throw new Exception("Escape recorded or kept the box.");

        const string inserted = "Vitela smoke insert";
        await BeginContentGestureAsync(_slots[0], 0, point, null!);
        _insertEditor!.Box.Text = inserted;
        await CommitInsertEditorAsync();
        await CommitInsertEditorAsync(); // idempotent: must not record twice
        if (_insertEditor is not null || !_annotationState!.CanUndo) throw new Exception("Commit did not record an undoable insertion.");
        await ApplyHistoryAsync(true);
        if (_annotationState!.CanUndo) throw new Exception("Commit recorded more than one command.");
        await ApplyHistoryAsync(false);

        // Empty box: a click that changed its mind records nothing.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(72, 80), null!);
        await CommitContentEditorAsync();
        await ApplyHistoryAsync(true);
        if (_annotationState!.CanUndo) throw new Exception("An empty insert box recorded a command.");
        await ApplyHistoryAsync(false);

        // --- an image to select, inserted at a known spot, then saved -----------
        var prepared = await _facade.PrepareImageInsertionAsync(_session!.SessionId, 0);
        var png = await TwoPixelPngAsync();
        var imageResult = await _facade.InsertContentImageAsync(_session.SessionId, prepared.Value!, png, 300, 500);
        if (!imageResult.IsSuccess) throw new Exception(imageResult.Error!.Message);
        var saved = await _facade.SaveToDestinationAsync(_session.SessionId,
            bytes => File.WriteAllBytesAsync(Path.Combine(output, "canvas-insert-fixture.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await ReopenForSmokeAsync(await File.ReadAllBytesAsync(Path.Combine(output, "canvas-insert-fixture.pdf")));

        var runs = await _facade.PageTextEditTargetsAsync(_session!.SessionId, 0);
        var run = runs.Value?.FirstOrDefault(candidate => candidate.Text == inserted)
            ?? throw new Exception("Inserted text did not survive save and reopen.");
        if (Math.Abs(run.Bounds.X - point.X) > 1) throw new Exception($"Inserted text landed at x={run.Bounds.X}, not at the click.");

        // --- image card --------------------------------------------------------
        SetContentEditMode(true);
        var content = await EnsurePageContentAsync(0) ?? throw new Exception("Page content did not load.");
        var image = content.Images.FirstOrDefault(candidate => Math.Abs(candidate.Bounds.X - 300) < 1
            && Math.Abs(candidate.Bounds.Y + candidate.Bounds.Height - 500) < 1)
            ?? throw new Exception("Inserted image is not at its click's top-left corner.");
        _selectedContentImage = image;
        RedrawContentImageSelection();
        await ProbeImageReplacementAsync(image);
        if (CurrentImageControls() != ImageControls.Ready || !DeleteImageButton.IsEnabled || !ReplaceImageButton.IsEnabled
            || _editImageHint.Text != ImageSelectedHint)
            throw new Exception($"Selected image card is {CurrentImageControls()}, not Ready.");

        var notes = "Insert image's click-to-picker route is not automated (native picker).";
        if (_session.PageCount > 1)
        {
            // Moves the revision without dropping page 0's cached content: the
            // selected handle is now stale, and Delete must re-read it.
            var other = await _facade.PrepareTextInsertionAsync(_session.SessionId, 1);
            var moved = await _facade.InsertTextRunAsync(_session.SessionId, other.Value!, "other page", 72, 72, 12);
            if (!moved.IsSuccess) throw new Exception(moved.Error!.Message);
            _annotationState = moved.Value;
            notes += " Delete re-resolved a selection made stale by an edit on page 2.";
        }

        await DeleteSelectedImageAsync();
        if (_selectedContentImage is not null || DeleteImageButton.IsEnabled) throw new Exception("Delete kept its selection.");
        var afterDelete = await _facade.PageImagesAsync(_session.SessionId, 0);
        if (afterDelete.Value!.Any(candidate => candidate.Id == image.Id && candidate.Bounds == image.Bounds))
            throw new Exception("Deleted image remains on the page.");
        await ApplyHistoryAsync(true);
        var restored = await _facade.PageImagesAsync(_session.SessionId, 0);
        var back = restored.Value!.FirstOrDefault(candidate => candidate.Bounds == image.Bounds)
            ?? throw new Exception("One undo did not restore the image.");

        // Synthetic pending edit: the card's widest refusal.
        _selectedContentImage = back;
        _pendingImageEdits.Add((back.PageIndex, back.Id));
        RedrawContentImageSelection();
        if (DeleteImageButton.IsEnabled || ReplaceImageButton.IsEnabled || _editImageHint.Text != ImagePendingEditHint)
            throw new Exception("A pending edit did not disable both image commands.");

        SetContentInsertMode(ContentInsertKind.Image);
        SetContentEditMode(false);
        if (_contentInsertKind is not null || InsertImageButton.IsChecked == true)
            throw new Exception("Leaving content editing did not disarm the insert kind.");
        return notes;
    }

    private async Task ReopenForSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Canvas insert smoke", bytes));
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
    }

    private static async Task<byte[]> TwoPixelPngAsync()
    {
        using var stream = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
        encoder.SetPixelData(BitmapPixelFormat.Rgba8, BitmapAlphaMode.Ignore, 2, 1, 96, 96, [255, 0, 0, 255, 0, 0, 255, 255]);
        await encoder.FlushAsync();
        stream.Seek(0);
        using var input = stream.AsStreamForRead();
        using var copy = new MemoryStream();
        await input.CopyToAsync(copy);
        return copy.ToArray();
    }
}
