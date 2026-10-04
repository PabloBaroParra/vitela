using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Pdf.Windows;

// Uses real WinUI, StorageFile and the native core; substitutes only the picker response.
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
            await _window.StampPlacementSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "stamp-smoke.log"), "PASS PNG/JPEG placement; selected image preview; geometry; undo/redo; save; cancel; corrupt/unsupported/unreadable files; stale session; permission/busy/organize/modal guards; control restoration. Native picker UI not automated.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "stamp-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task StampPlacementSmokeAsync(string output)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (Content.XamlRoot is null)
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("Window did not load.");
            await Task.Delay(50);
        }
        await OpenDocumentAsync("Stamp smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        var rect = new PdfCoreRect(40, 80, 100, 40);
        var before = _annotationState!;

        async Task unchanged(Func<Task<StorageFile?>> pick)
        {
            var state = _annotationState!;
            await PlaceImageStampAsync(0, rect, pick);
            var after = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
            if (!after.Annotations.SequenceEqual(state.Annotations) || after.CanUndo != state.CanUndo || after.CanRedo != state.CanRedo)
                throw new Exception("Rejected input changed annotations or history.");
            if (_isBusy || !StampButton.IsEnabled) throw new Exception("Controls were not restored.");
        }

        await unchanged(() => Task.FromResult<StorageFile?>(null));
        await unchanged(() => throw new IOException("Picker/read failure"));
        foreach (var bytes in new[] { "not an image"u8.ToArray(), new byte[] { 137, 80, 78, 71, 13, 10, 26, 10 } })
        {
            var path = Path.Combine(output, "invalid-stamp.png");
            await File.WriteAllBytesAsync(path, bytes);
            await unchanged(async () => await StorageFile.GetFileFromPathAsync(path));
        }
        var missingPath = Path.Combine(output, "unreadable-stamp.png");
        await File.WriteAllBytesAsync(missingPath, [1]);
        var missing = await StorageFile.GetFileFromPathAsync(missingPath);
        File.Delete(missingPath);
        await unchanged(() => Task.FromResult<StorageFile?>(missing));

        foreach (var format in new[] { BitmapEncoder.PngEncoderId, BitmapEncoder.JpegEncoderId })
        {
            var path = Path.Combine(output, format == BitmapEncoder.PngEncoderId ? "chosen-stamp.png" : "chosen-stamp.jpg");
            using (var stream = new InMemoryRandomAccessStream())
            {
                var encoder = await BitmapEncoder.CreateAsync(format, stream);
                encoder.SetPixelData(BitmapPixelFormat.Rgba8, BitmapAlphaMode.Ignore, 2, 1, 96, 96,
                    [255, 0, 0, 255, 0, 0, 255, 255]);
                await encoder.FlushAsync();
                stream.Seek(0);
                using var input = stream.AsStreamForRead();
                using var destination = File.Create(path);
                await input.CopyToAsync(destination);
            }
            await PlaceImageStampAsync(0, rect, async () =>
            {
                if (!_isBusy || StampButton.IsEnabled || UndoButton.IsEnabled) throw new Exception("Picker did not lock editing.");
                return await StorageFile.GetFileFromPathAsync(path);
            });
            var stamp = _annotationState!.Annotations[^1];
            if (stamp.Kind != AnnotationKind.Stamp || stamp.Rect != new AnnotationRect(40, 80, 100, 40)
                || _selectedAnnotationId != stamp.Id || !_stampPreviews.TryGet(stamp.Id, out _) || _isBusy || !StampButton.IsEnabled)
                throw new Exception("Selected image did not reach the stamp and preview path.");
            await ApplyHistoryAsync(true);
            if (_annotationState!.Annotations.Count != before.Annotations.Count || _annotationState.CanUndo != before.CanUndo)
                throw new Exception("Stamp was not one undo step.");
            await ApplyHistoryAsync(false);
            if (_annotationState!.Annotations.Count != before.Annotations.Count + 1) throw new Exception("Redo lost stamp.");
            await ApplyHistoryAsync(true);
        }

        var changedResponse = new TaskCompletionSource<StorageFile?>();
        var changedPlacement = PlaceImageStampAsync(0, rect, () => changedResponse.Task);
        _annotationState = _annotationState! with { EditingAllowed = false };
        changedResponse.SetResult(await StorageFile.GetFileFromPathAsync(Path.Combine(output, "chosen-stamp.png")));
        await changedPlacement;
        if (_annotationState.CanUndo || _annotationState.Annotations.Count != before.Annotations.Count)
            throw new Exception("Changed snapshot accepted an old placement.");
        await RefreshAnnotationStateAsync();

        // A response that arrives after the document was replaced must be ignored.
        var fixtureSave = await _facade.SaveToDestinationAsync(_session!.SessionId,
            bytes => File.WriteAllBytesAsync(Path.Combine(output, "stamp-history-fixture.pdf"), bytes));
        if (!fixtureSave.IsSuccess) throw new Exception(fixtureSave.Error!.Message);
        var response = new TaskCompletionSource<StorageFile?>();
        var placement = PlaceImageStampAsync(0, rect, () => response.Task);
        var replacement = await _facade.OpenAsync(new DocumentSource("Replacement", await File.ReadAllBytesAsync(SamplePath)));
        if (!replacement.IsSuccess) throw new Exception(replacement.Error!.Message);
        ShowOpenedDocument(replacement.Value!);
        await RefreshAnnotationStateAsync();
        response.SetResult(await StorageFile.GetFileFromPathAsync(Path.Combine(output, "chosen-stamp.png")));
        await placement;
        if (_annotationState!.Annotations.Count != 0 || _annotationState.CanUndo || _isBusy || !StampButton.IsEnabled)
            throw new Exception("Stale response changed replacement or left it disabled.");

        var pickerCalls = 0;
        Task<StorageFile?> unexpectedPicker()
        {
            pickerCalls++;
            return Task.FromResult<StorageFile?>(null);
        }
        _annotationState = _annotationState with { EditingAllowed = false };
        await PlaceImageStampAsync(0, rect, unexpectedPicker);
        _annotationState = _annotationState with { EditingAllowed = true };
        _isBusy = true;
        await PlaceImageStampAsync(0, rect, unexpectedPicker);
        _isBusy = false;
        _organizing = true;
        await PlaceImageStampAsync(0, rect, unexpectedPicker);
        _organizing = false;
        _dialogOpen = true;
        await PlaceImageStampAsync(0, rect, unexpectedPicker);
        _dialogOpen = false;
        if (pickerCalls != 0) throw new Exception("Guard allowed picker to open.");
        await PlaceImageStampAsync(0, rect, async () => await StorageFile.GetFileFromPathAsync(Path.Combine(output, "chosen-stamp.png")));
        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId,
            bytes => File.WriteAllBytesAsync(Path.Combine(output, "stamp-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
    }
}
