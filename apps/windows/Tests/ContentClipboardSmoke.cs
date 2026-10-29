using System.Text;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness (B25 T-209), the same replacement-entry-point
// pattern as the other smoke tests: a client box on page 1 copied and cut
// onto page 2 — whose own /F1 is a different font — through the real canvas
// code and the real core.
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
            await _window.ContentClipboardSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "content-clipboard-smoke.log"),
                "PASS a marquee and Ctrl+C copy the client box and leave page 1 alone; Ctrl+V pastes it on page 2 with "
                + "its top-left at the pointer and selects what it added; a drag moves the whole paste and one undo takes "
                + "it all back; the save reopens with the box on page 2 next to page 2's own Courier text; Ctrl+X takes "
                + "the box off page 1 in one undo step and pastes it back.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "content-clipboard-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ContentClipboardSmokeAsync(string output)
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenClipboardSmokeAsync(TwoPagePdf());
        SetContentEditMode(true);

        // Copy: a marquee over the box, then Ctrl+C.
        await ClipboardMarqueeAsync(0, new AnnotationPoint(250, 700), new AnnotationPoint(520, 590));
        if (_contentSelection is not { Count: 3 } boxed) throw new Exception($"The marquee selected {_contentSelection?.Count} items. Status: {AnnotationStatus.Text}");
        if (!TryCopyContent()) throw new Exception("Ctrl+C did not take the selection.");
        await WaitIdleAsync();
        if (_contentClip is null) throw new Exception($"Nothing was copied. Status: {AnnotationStatus.Text}");
        RequireTexts(await TextsAsync(0), ["Cliente", "Domingo", "Pag 1"], "page 1 after the copy");

        // Paste on page 2 at the pointer: the box's top-left lands there.
        TrackContentPointer(1, new AnnotationPoint(50, 400));
        if (!TryPasteContent()) throw new Exception("Ctrl+V did not take the clip.");
        await WaitIdleAsync();
        RequireTexts(await TextsAsync(1), ["Pie", "Cliente", "Domingo"], "page 2 after the paste");
        RequireFrame(await FrameAsync(1), 50, 400, "the pasted frame");
        if (_contentSelection is not { PageIndex: 1, Count: 3 })
            throw new Exception($"The paste selected {_contentSelection?.Count} items on page {_contentSelection?.PageIndex}. Status: {AnnotationStatus.Text}");

        // A drag moves the whole paste; one undo takes all of it back.
        await ClipboardDragAsync(1, new AnnotationPoint(230, 330), new AnnotationPoint(260, 330));
        RequireFrame(await FrameAsync(1), 80, 400, "the dragged paste");
        var file = Path.Combine(output, "content-clipboard-fixture.pdf");
        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId, bytes => File.WriteAllBytesAsync(file, bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await ApplyHistoryAsync(true);
        RequireTexts(await TextsAsync(1), ["Pie"], "page 2 after one undo");

        await OpenClipboardSmokeAsync(await File.ReadAllBytesAsync(file));
        RequireTexts(await TextsAsync(1), ["Pie", "Cliente", "Domingo"], "page 2 in the reopened save");
        RequireFrame(await FrameAsync(1), 80, 400, "the saved frame");
        var page2 = (await _facade.PageContentAsync(_session!.SessionId, 1)).Value!;
        if (page2.TextRuns.Single(run => run.Text == "Pie").BaseFont is { } pie && !pie.Contains("Courier"))
            throw new Exception($"Page 2's own text changed font to {pie}.");

        // Cut: page 1 loses the box in one undo step, and the clip pastes back.
        SetContentEditMode(true);
        await ClipboardMarqueeAsync(0, new AnnotationPoint(250, 700), new AnnotationPoint(520, 590));
        await CopyContentAsync(_contentSelection!, cut: true);
        RequireTexts(await TextsAsync(0), ["Pag 1"], "page 1 after the cut");
        TrackContentPointer(0, new AnnotationPoint(300, 300));
        await PasteContentAsync(_contentClip!);
        RequireFrame(await FrameAsync(0), 300, 300, "the box pasted back");
        await ApplyHistoryAsync(true);
        await ApplyHistoryAsync(true);
        RequireTexts(await TextsAsync(0), ["Cliente", "Domingo", "Pag 1"], "page 1 after undoing the paste and the cut");
    }

    private async Task WaitIdleAsync()
    {
        for (var attempt = 0; _isBusy && attempt < 200; attempt++) await Task.Delay(25);
        if (_isBusy) throw new Exception("The clipboard edit never finished.");
    }

    private async Task ClipboardMarqueeAsync(int page, AnnotationPoint from, AnnotationPoint to)
    {
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[page], page, from, null!);
        if (_selectionGesture is not { Kind: SelectionGestureKind.Marquee })
            throw new Exception($"A press at {from} did not start a marquee. Status: {AnnotationStatus.Text}");
        ContinueContentSelectionGesture(_slots[page], page, to);
        _contentPointerDown = false;
        await CompleteContentSelectionGestureAsync(to, dragging: true);
    }

    private async Task ClipboardDragAsync(int page, AnnotationPoint from, AnnotationPoint to)
    {
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[page], page, from, null!);
        if (_selectionGesture is not { Kind: SelectionGestureKind.Drag })
            throw new Exception($"A press at {from} inside the paste did not start a drag. Status: {AnnotationStatus.Text}");
        ContinueContentSelectionGesture(_slots[page], page, to);
        _contentPointerDown = false;
        await CompleteContentSelectionGestureAsync(to, dragging: true);
    }

    private async Task<string[]> TextsAsync(uint page) =>
        [.. (await _facade.PageContentAsync(_session!.SessionId, page)).Value!.TextBlocks.Select(block => block.Text)];

    private async Task<AnnotationRect?> FrameAsync(uint page) =>
        (await _facade.PageContentAsync(_session!.SessionId, page)).Value!.Paths.SingleOrDefault()?.Bounds;

    private void RequireTexts(string[] actual, string[] expected, string when)
    {
        if (!actual.Order().SequenceEqual(expected.Order()))
            throw new Exception($"The text on {when} is [{string.Join(", ", actual)}], not [{string.Join(", ", expected)}]. Status: {AnnotationStatus.Text}");
    }

    private void RequireFrame(AnnotationRect? frame, double left, double top, string what)
    {
        if (frame is null || Math.Abs(frame.X - left) > 0.05 || Math.Abs(frame.Y + frame.Height - top) > 0.05)
            throw new Exception($"{what} is {frame}, not top-left ({left}, {top}). Status: {AnnotationStatus.Text}");
    }

    private async Task OpenClipboardSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Content clipboard smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count < 2 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count < 2) throw new Exception("Pages were not laid out.");
    }

    /// <summary>
    /// Page 1: a client box (a stroked frame with two lines inside) and a
    /// footer. Page 2: one word in its own /F1, which is Courier.
    /// </summary>
    private static byte[] TwoPagePdf()
    {
        const string first = "0.5 w 300 600 200 80 re S "
            + "BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -20 Td (Domingo) Tj ET "
            + "BT /F1 10 Tf 300 40 Td (Pag 1) Tj ET";
        const string second = "BT /F1 10 Tf 50 50 Td (Pie) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /Font << /F1 7 0 R >> >> >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 8 0 R >> >> >>",
            $"<< /Length {first.Length} >>\nstream\n{first}\nendstream",
            $"<< /Length {second.Length} >>\nstream\n{second}\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
        ];
        var pdf = new StringBuilder("%PDF-1.7\n");
        var offsets = new List<int>();
        for (var index = 0; index < objects.Length; index++)
        {
            offsets.Add(pdf.Length);
            pdf.Append($"{index + 1} 0 obj\n{objects[index]}\nendobj\n");
        }
        var xref = pdf.Length;
        pdf.Append($"xref\n0 {objects.Length + 1}\n0000000000 65535 f \n");
        foreach (var offset in offsets) pdf.Append($"{offset:D10} 00000 n \n");
        pdf.Append($"trailer\n<< /Size {objects.Length + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n");
        return Encoding.ASCII.GetBytes(pdf.ToString());
    }
}
