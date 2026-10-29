using System.Text;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness (B25 T-208), the same replacement-entry-point
// pattern as the other smoke tests: a client box (a stroked frame with two
// lines inside), a logo beside it and a page footer, driven through the real
// canvas gesture code and the real core.
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
            await _window.ContentSelectionSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "content-selection-smoke.log"),
                "PASS a marquee from empty page gathers the frame and both lines it touches and not the logo or footer; "
                + "SHIFT+click adds the logo and takes it out again; a drag inside the selection moves all of it, which "
                + "stays selected where it landed; a second drag folds into the first, so one undo takes both back; the "
                + "save reopens with every member moved; a marquee over the page and Delete empty it in one undo step; "
                + "a click on empty page selects nothing.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "content-selection-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    private static readonly AnnotationPoint EmptyAboveTable = new(250, 700);

    internal async Task ContentSelectionSmokeAsync(string output)
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenSelectionSmokeAsync(TablePdf());
        SetContentEditMode(true);
        var original = await PageSnapshotAsync();

        // A marquee from empty page across the client box.
        await MarqueeAsync(EmptyAboveTable, new AnnotationPoint(520, 590));
        RequireSelection("after the marquee", blocks: 2, images: 0, paths: 1);

        // SHIFT+click on the logo adds it; a second one takes it out.
        _contentPressShift = true;
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(130, 640), null!);
        RequireSelection("after SHIFT+click on the logo", blocks: 2, images: 1, paths: 1);
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(130, 640), null!);
        RequireSelection("after a second SHIFT+click", blocks: 2, images: 0, paths: 1);
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(130, 640), null!);
        _contentPressShift = false;

        // A drag inside the selection — on an empty cell — moves all of it.
        await DragSelectionAsync(new AnnotationPoint(450, 610), new AnnotationPoint(450, 410));
        RequireSelection("after the drag", blocks: 2, images: 1, paths: 1);
        var once = await PageSnapshotAsync();
        RequireShifted(original, once, dx: 0, dy: -200, footerMoves: false, "after the drag");

        // Dragged again before a save: one undo step for both.
        await DragSelectionAsync(new AnnotationPoint(450, 410), new AnnotationPoint(480, 410));
        var twice = await PageSnapshotAsync();
        RequireShifted(original, twice, dx: 30, dy: -200, footerMoves: false, "after the second drag");

        var file = Path.Combine(output, "content-selection-fixture.pdf");
        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId, bytes => File.WriteAllBytesAsync(file, bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await ApplyHistoryAsync(true);
        RequireShifted(original, await PageSnapshotAsync(), dx: 0, dy: 0, footerMoves: false, "after one undo");

        await OpenSelectionSmokeAsync(await File.ReadAllBytesAsync(file));
        RequireShifted(original, await PageSnapshotAsync(), dx: 30, dy: -200, footerMoves: false, "in the reopened save");

        // A marquee over the whole page, then Delete: one step empties it.
        await OpenSelectionSmokeAsync(TablePdf());
        SetContentEditMode(true);
        await MarqueeAsync(new AnnotationPoint(5, 787), new AnnotationPoint(607, 5));
        RequireSelection("over the whole page", blocks: 3, images: 1, paths: 1);
        await DeleteContentSelectionAsync();
        var emptied = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!;
        if (emptied.TextRuns.Count + emptied.Images.Count + emptied.Paths.Count != 0)
            throw new Exception($"Delete left items on the page. Status: {AnnotationStatus.Text}");
        if (_contentSelection is not null) throw new Exception("The deleted selection stayed selected.");
        await ApplyHistoryAsync(true);
        RequireShifted(original, await PageSnapshotAsync(), dx: 0, dy: 0, footerMoves: false, "after undoing Delete");

        // A click on empty page selects nothing.
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, EmptyAboveTable, null!);
        _contentPointerDown = false;
        await CompleteContentSelectionGestureAsync(EmptyAboveTable, dragging: false);
        if (_contentSelection is not null) throw new Exception("A click on empty page selected something.");
    }

    private async Task MarqueeAsync(AnnotationPoint from, AnnotationPoint to)
    {
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, from, null!);
        if (_selectionGesture is not { Kind: SelectionGestureKind.Marquee })
            throw new Exception($"A press at {from} did not start a marquee. Status: {AnnotationStatus.Text}");
        ContinueContentSelectionGesture(_slots[0], 0, to);
        _contentPointerDown = false;
        await CompleteContentSelectionGestureAsync(to, dragging: true);
    }

    private async Task DragSelectionAsync(AnnotationPoint from, AnnotationPoint to)
    {
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, from, null!);
        if (_selectionGesture is not { Kind: SelectionGestureKind.Drag })
            throw new Exception($"A press at {from} inside the selection did not start a drag. Status: {AnnotationStatus.Text}");
        ContinueContentSelectionGesture(_slots[0], 0, to);
        _contentPointerDown = false;
        await CompleteContentSelectionGestureAsync(to, dragging: true);
    }

    private void RequireSelection(string when, int blocks, int images, int paths)
    {
        if (_contentSelection is not { } selection
            || selection.Blocks.Count != blocks || selection.Images.Count != images || selection.Paths.Count != paths)
        {
            throw new Exception($"The selection {when} is {_contentSelection?.Blocks.Count}/{_contentSelection?.Images.Count}/"
                + $"{_contentSelection?.Paths.Count}, not {blocks}/{images}/{paths}. Status: {AnnotationStatus.Text}");
        }
    }

    private sealed record PageSnapshot(AnnotationRect Cliente, AnnotationRect Domingo, AnnotationRect Footer,
        AnnotationRect Logo, AnnotationRect Frame);

    private async Task<PageSnapshot> PageSnapshotAsync()
    {
        var content = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!;
        AnnotationRect Text(string text) => content.TextBlocks.Single(block => block.Text == text).Bounds;
        return new(Text("Cliente"), Text("Domingo"), Text("Pag 1"), content.Images.Single().Bounds, content.Paths.Single().Bounds);
    }

    private static void RequireShifted(PageSnapshot from, PageSnapshot to, double dx, double dy, bool footerMoves, string when)
    {
        static bool Shifted(AnnotationRect a, AnnotationRect b, double dx, double dy) =>
            Math.Abs(b.X - (a.X + dx)) < 0.05 && Math.Abs(b.Y - (a.Y + dy)) < 0.05;
        var pairs = new[] { (from.Cliente, to.Cliente), (from.Domingo, to.Domingo), (from.Logo, to.Logo), (from.Frame, to.Frame) };
        if (!pairs.All(pair => Shifted(pair.Item1, pair.Item2, dx, dy)))
            throw new Exception($"The members {when} are {to}, not {from} shifted by ({dx}, {dy}).");
        if (!Shifted(from.Footer, to.Footer, footerMoves ? dx : 0, footerMoves ? dy : 0))
            throw new Exception($"The footer {when} is {to.Footer}, not {from.Footer}.");
    }

    private async Task OpenSelectionSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Content selection smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
    }

    /// <summary>A client box — a stroked frame with two lines inside — a logo to its left and a footer.</summary>
    private static byte[] TablePdf()
    {
        const string content = "0.5 w 300 600 200 80 re S "
            + "BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -20 Td (Domingo) Tj ET "
            + "q 60 0 0 40 100 620 cm /Im1 Do Q "
            + "BT /F1 10 Tf 300 40 Td (Pag 1) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> >>",
            $"<< /Length {content.Length} >>\nstream\n{content}\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
            "<< /Type /XObject /Subtype /Image /Width 1 /Height 1 /ColorSpace /DeviceGray /BitsPerComponent 8 /Length 1 >>\nstream\nx\nendstream",
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
