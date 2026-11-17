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
                + "the box off page 1 in one undo step and pastes it back; a lone pasted image is selected as an image "
                + "and replaced twice before a save, which writes.");
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

        // A lone pasted image is selected as an image, and replaced twice
        // before a save: the second through the core's checkpoint.
        await ClipboardMarqueeAsync(0, new AnnotationPoint(90, 670), new AnnotationPoint(170, 610));
        if (_contentSelection is not { Count: 1, Images.Count: 1 })
            throw new Exception($"The marquee over the logo selected {_contentSelection?.Count} items. Status: {AnnotationStatus.Text}");
        await CopyContentAsync(_contentSelection!, cut: false);
        TrackContentPointer(1, new AnnotationPoint(300, 300));
        await PasteContentAsync(_contentClip!);
        if (_contentSelection is not null || _selectedContentImage is not { PageIndex: 1 })
            throw new Exception($"The pasted logo is not selected as an image. Status: {AnnotationStatus.Text}");
        if (!ReplaceImageButton.IsEnabled) throw new Exception($"Replace is not offered for the pasted logo. Status: {AnnotationStatus.Text}");
        foreach (var picture in new[] { RedPng, RedPng })
        {
            await ReplaceSelectedImageAsync(_ => Task.FromResult<byte[]?>(picture));
            if (!AnnotationStatus.Text.StartsWith("Image replaced", StringComparison.Ordinal))
                throw new Exception($"Replacing the pasted logo failed: {AnnotationStatus.Text}");
            if (_selectedContentImage is not { PageIndex: 1 }) throw new Exception("The replaced logo lost its selection.");
            if (!ReplaceImageButton.IsEnabled) throw new Exception($"Replace is not offered again. Status: {AnnotationStatus.Text}");
        }
        var images = (await _facade.PageImagesAsync(_session!.SessionId, 1)).Value!;
        if (images.Count != 1 || Math.Abs(images[0].Bounds.X - 300) > 0.05)
            throw new Exception($"Page 2 holds {images.Count} images after two replacements.");
        var replaced = Path.Combine(output, "content-clipboard-replaced.pdf");
        var kept = await _facade.SaveToDestinationAsync(_session!.SessionId, bytes => File.WriteAllBytesAsync(replaced, bytes));
        if (!kept.IsSuccess) throw new Exception(kept.Error!.Message);
    }

    /// <summary>A 2x3 PNG, the core fixtures' replacement picture.</summary>
    private static readonly byte[] RedPng =
    [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x03, 0x08, 0x02, 0x00, 0x00, 0x00, 0x36,
        0x88, 0x49, 0xd6, 0x00, 0x00, 0x00, 0x12, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xe0,
        0x8a, 0x3a, 0x71, 0x22, 0x8a, 0x8b, 0x01, 0x85, 0x02, 0x00, 0x49, 0xe9, 0x07, 0x09, 0x02,
        0xc1, 0xa0, 0x0c, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

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
    /// Page 1: a client box (a stroked frame with two lines inside), a
    /// footer and a one-pixel logo left of the box. Page 2: one word in its own /F1, which is Courier.
    /// </summary>
    private static byte[] TwoPagePdf()
    {
        const string first = "0.5 w 300 600 200 80 re S "
            // 30pt apart, past the 2.4 em a paragraph allows: two blocks, not one.
            + "BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -30 Td (Domingo) Tj ET "
            + "BT /F1 10 Tf 300 40 Td (Pag 1) Tj ET "
            + "q 60 0 0 40 100 620 cm /Im1 Do Q";
        const string second = "BT /F1 10 Tf 50 50 Td (Pie) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 5 0 R /Resources << /Font << /F1 7 0 R >> /XObject << /Im1 9 0 R >> >> >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 8 0 R >> >> >>",
            $"<< /Length {first.Length} >>\nstream\n{first}\nendstream",
            $"<< /Length {second.Length} >>\nstream\n{second}\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>",
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Length 3 >>\nstream\nd\u001e\u001e\nendstream",
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
