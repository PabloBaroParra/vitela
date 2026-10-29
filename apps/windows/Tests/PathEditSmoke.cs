using System.Text;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness (B25 T-206), the same replacement-entry-point
// pattern as the other smoke tests: a table frame and a rule driven through the
// real canvas gesture code and the real core, then saved and reopened.
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
            await _window.PathEditSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "path-edit-smoke.log"),
                "PASS a click inside the frame selects its text; a click on its border selects it; a drag moves it; "
                + "a corner drag right after resizes it without a save in between; Delete removes the rule; "
                + "one undo restores it; the save reopens with the moved, resized frame and the rule.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "path-edit-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task PathEditSmokeAsync(string output)
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenPathSmokeAsync(TablePdf());
        SetContentEditMode(true);
        var content = await EnsurePageContentAsync(0) ?? throw new Exception("Page content did not load.");
        if (content.Paths.Count != 2) throw new Exception($"Expected the frame and the rule, read {content.Paths.Count} paths.");

        // Inside the frame, on its text: the text, not the frame.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(335, 243), null!);
        if (_selectedContentPath is not null || _selectedTextBlock is null)
            throw new Exception("A click on the cell text did not select the text over the frame.");

        // On the border: selected, and dragged 20pt right and down.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(319.5, 200), null!);
        if (_selectedContentPath?.Bounds != new AnnotationRect(319, 135, 251, 125) || _contentPathDrag is null)
            throw new Exception("A click on the border did not select the frame.");
        await CompleteContentPathDragAsync(new AnnotationPoint(339.5, 180));
        RequireFrame(new AnnotationRect(339, 115, 251, 125), "after the move");

        // Straight away, no save: the top-right corner grows it.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(590, 240), null!);
        if (_contentPathDrag?.Corner != Corner.TopRight) throw new Exception("The corner handle did not start a resize.");
        await CompleteContentPathDragAsync(new AnnotationPoint(610, 260));
        RequireFrame(new AnnotationRect(339, 115, 271, 145), "after the resize");

        // The rule: selected on its line, deleted, and one undo brings it back.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(400, 100.5), null!);
        await CompleteContentPathDragAsync(new AnnotationPoint(400, 100.5));
        if (_selectedContentPath?.Bounds.Height != 0) throw new Exception("A click on the rule did not select it.");
        if (!_deleteShapeButton.IsEnabled) throw new Exception("Delete shape is not available for a selected rule.");
        await DeleteSelectedPathAsync();
        if (await PathCountAsync() != 1) throw new Exception("Delete did not remove the rule.");
        await ApplyHistoryAsync(true);
        if (await PathCountAsync() != 2) throw new Exception("One undo did not restore the rule.");

        var file = Path.Combine(output, "path-edit-fixture.pdf");
        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId, bytes => File.WriteAllBytesAsync(file, bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await OpenPathSmokeAsync(await File.ReadAllBytesAsync(file));
        var reopened = (await _facade.PagePathsAsync(_session!.SessionId, 0)).Value!;
        if (!reopened.Any(path => Near(path.Bounds, new AnnotationRect(339, 115, 271, 145))) || reopened.Count != 2)
            throw new Exception("The save did not keep the moved, resized frame and the rule.");
    }

    private void RequireFrame(AnnotationRect expected, string when)
    {
        if (_selectedContentPath is not { } selected || !Near(selected.Bounds, expected))
            throw new Exception($"Frame {when} is {_selectedContentPath?.Bounds}, not {expected}. Status: {AnnotationStatus.Text}");
    }

    private async Task<int> PathCountAsync() => (await _facade.PagePathsAsync(_session!.SessionId, 0)).Value!.Count;

    private static bool Near(AnnotationRect left, AnnotationRect right) =>
        Math.Abs(left.X - right.X) < 0.01 && Math.Abs(left.Y - right.Y) < 0.01
        && Math.Abs(left.Width - right.Width) < 0.01 && Math.Abs(left.Height - right.Height) < 0.01;

    private async Task OpenPathSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Path edit smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
    }

    /// <summary>A one-page PDF shaped like the reference quote's client box.</summary>
    private static byte[] TablePdf()
    {
        const string content = "0.5 w 319 135 251 125 re S\n319 100 m 570 100 l S\nBT /F1 12 Tf 330 240 Td (CLIENTE) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
            $"<< /Length {content.Length} >>\nstream\n{content}\nendstream",
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>",
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
