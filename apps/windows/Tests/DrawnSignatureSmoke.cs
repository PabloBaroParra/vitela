using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

// Uses real WinUI and the native core. The pad dialog's pointer drawing and the real
// ContentDialogs are not automated: the pad is covered by the stroke -> PNG unit tests
// and a manual check.
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
            await _window.DrawnSignatureSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "signature-smoke.log"), "PASS arming releases other tools; stale session arms nothing; proportional placement through the core; selected preview; one undo step; leaving Sign / choosing a tool disarms; control gating; save. Pad pointer drawing and ContentDialogs not automated.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "signature-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task DrawnSignatureSmokeAsync(string output)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (Content.XamlRoot is null)
        {
            if (DateTime.UtcNow >= deadline) throw new TimeoutException("Window did not load.");
            await Task.Delay(50);
        }
        await OpenDocumentAsync("Signature smoke", await File.ReadAllBytesAsync(SamplePath));
        if (_session is null) throw new Exception("Sample did not open.");
        await RefreshAnnotationStateAsync();
        var sessionId = _session.SessionId;
        if (!_drawSignatureButton.IsEnabled) throw new Exception("Draw signature is not enabled on an editable document.");

        // A wide signature: 4:1-ish ink, so a stretched placement would be visible.
        List<List<PadPoint>> strokes = [[new(10, 10), new(210, 10)], [new(10, 40), new(110, 50)]];
        var png = SignaturePng.Render(strokes, SignaturePadView.StrokeWidth) ?? throw new Exception("No PNG for a drawn signature.");
        var frame = DrawnSignature.Frame(strokes, SignaturePadView.StrokeWidth)!.Value;

        // Arming claims the next click: it replaces any armed tool and leaves content editing.
        Arm(AnnotationKind.Highlight);
        SetContentEditMode(true);
        ArmDrawnSignature(sessionId, png);
        if (_armedAnnotation != AnnotationKind.Stamp || _armedStampImage != png || StampButton.IsChecked != true
            || HighlightButton.IsChecked == true || _contentEditMode)
            throw new Exception("Arming did not release the other tools and arm the stamp.");

        // Choosing a tool by hand drops the armed picture.
        Arm(null);
        if (_armedStampImage is not null || StampButton.IsChecked == true) throw new Exception("Pointer mode kept the signature armed.");

        // A PNG that arrives for another document arms nothing.
        ArmDrawnSignature("not-the-open-session", png);
        if (_armedAnnotation is not null || _armedStampImage is not null) throw new Exception("A stale session armed the signature.");

        // Leaving the Sign page disarms; staying on it does not.
        SelectToolPage("Sign");
        ArmDrawnSignature(sessionId, png);
        SelectToolPage("Sign");
        if (_armedStampImage is null) throw new Exception("Staying on Sign disarmed the signature.");
        SelectToolPage("Annotate");
        if (_armedStampImage is not null || _armedAnnotation is not null) throw new Exception("Leaving Sign kept the signature armed.");

        // The click places it through the core's stamp placement: proportions kept, one undo step, picture shown.
        var before = _annotationState!;
        ArmDrawnSignature(sessionId, png);
        await PlaceArmedStampAsync(0, new AnnotationPoint(100, 600), png);
        var stamp = _annotationState!.Annotations[^1];
        if (stamp.Kind != AnnotationKind.Stamp || stamp.Rect is not { } rect) throw new Exception("No stamp was placed.");
        var expected = (double)frame.PixelWidth / frame.PixelHeight;
        if (Math.Abs(rect.Width / rect.Height - expected) > 0.05 * expected)
            throw new Exception($"Placement stretched the signature: {rect.Width}x{rect.Height} for a {expected:F2} PNG.");
        if (Math.Abs(rect.X - 100) > 0.01 || _selectedAnnotationId != stamp.Id || !_stampPreviews.TryGet(stamp.Id, out _) || _isBusy)
            throw new Exception("The placed signature is not selected with its preview at the click.");
        await ApplyHistoryAsync(true);
        if (_annotationState!.Annotations.Count != before.Annotations.Count) throw new Exception("The signature was not one undo step.");
        await ApplyHistoryAsync(false);

        // Placement is refused where a stamp is not allowed.
        var count = _annotationState!.Annotations.Count;
        _isBusy = true;
        await PlaceArmedStampAsync(0, new AnnotationPoint(100, 500), png);
        _isBusy = false;
        _organizing = true;
        await PlaceArmedStampAsync(0, new AnnotationPoint(100, 500), png);
        _organizing = false;
        if (_annotationState!.Annotations.Count != count) throw new Exception("A guard let the signature through.");

        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId,
            bytes => File.WriteAllBytesAsync(Path.Combine(output, "signature-smoke.pdf"), bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
    }
}
