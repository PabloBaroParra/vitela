using System.Text;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native runtime harness (B25 T-207), the same replacement-entry-point
// pattern as the other smoke tests: a heading written as three show-text
// operators, driven through the real canvas gesture code and the real core.
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
            await _window.TextBlockSmokeAsync(output);
            File.WriteAllText(Path.Combine(output, "text-block-smoke.log"),
                "PASS the split heading reads as one block beside a separate column; a drag moves the whole block "
                + "staying selected where it landed, Delete on it folds into the move, and one undo takes both back; a click selects the block and a "
                + "double-click opens one editor over it; typing in two writes is one undo step; Delete on the "
                + "selection, and Delete text in a block editor, remove the block and one undo restores it; the save "
                + "reopens with the retyped block as one run; typed-in and retyped one-run text can be selected end to end and deleted before a save; an image dragged twice before a save keeps its frame on it and "
                + "one undo puts it back.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "text-block-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    private static readonly AnnotationPoint OnHeading = new(150, 703);

    internal async Task TextBlockSmokeAsync(string output)
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");
        await OpenTextBlockSmokeAsync(HeadingPdf());
        SetContentEditMode(true);
        await RequireBlocksAsync("on open", "PRESUPUESTO N 3.725/1", "Total");

        // A press that travels: the whole block moves, as one undo step.
        var before = await HeadingBoundsAsync();
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, OnHeading, null!);
        if (_textBlockDrag?.Block.Runs.Count != 3) throw new Exception("A press on the heading did not take its whole block.");
        var dropped = new AnnotationPoint(OnHeading.X, OnHeading.Y - 100);
        ContinueTextBlockGesture(_slots[0], 0, dropped);
        if (_textBlockDrag is not { Dragging: true }) throw new Exception("Travelling past the threshold did not start a drag.");
        _contentPointerDown = false;
        await CompleteTextBlockGestureAsync(dropped, dragging: true);
        var moved = await HeadingBoundsAsync();
        if (Math.Abs(moved.Y - (before.Y - 100)) > 0.01 || Math.Abs(moved.X - before.X) > 0.01)
            throw new Exception($"The block is at {moved}, not 100pt below {before}. Status: {AnnotationStatus.Text}");
        if (_selectedTextBlock is not { } landed || Math.Abs(landed.Bounds.Y - moved.Y) > 0.01)
            throw new Exception("The moved block did not stay selected where it landed.");

        // Delete on the block it just moved: folds into the queued move.
        await DeleteSelectedTextBlockAsync();
        await RequireBlocksAsync("after deleting the moved block", "Total");
        await ApplyHistoryAsync(true);
        if (Math.Abs((await HeadingBoundsAsync()).Y - before.Y) > 0.01)
            throw new Exception("One undo did not take back both the move and the delete.");

        // A click selects the block and opens nothing.
        await BeginContentGestureAsync(_slots[0], 0, OnHeading, null!);
        if (_selectedTextBlock?.Runs.Count != 3 || _pump.Box is not null || !DeleteTextButton.IsEnabled)
            throw new Exception($"A click did not select the whole block. Status: {AnnotationStatus.Text}");

        // A double-click: one editor over the block, holding the block's text.
        await OpenContentEditorOnDoubleTapAsync(0, OnHeading);
        if (_selectedTextBlock is not null
            || _pump.Box is not { Block: { } block } editor || editor.Box.Text != "PRESUPUESTO N 3.725/1" || block.Runs.Count != 3)
            throw new Exception($"A double-click did not open a block editor. Status: {AnnotationStatus.Text}");

        // Two writes while typing fold into one step.
        editor.Box.Text = "FACT";
        await _pump.PumpAsync();
        editor.Box.Text = "FACTURA 12";
        await CommitContentEditorAsync();
        await RequireBlocksAsync("after the retype", "FACTURA 12", "Total");
        await ApplyHistoryAsync(true);
        await RequireBlocksAsync("after one undo", "PRESUPUESTO N 3.725/1", "Total");
        await ApplyHistoryAsync(false);
        await RequireBlocksAsync("after the redo", "FACTURA 12", "Total");

        var file = Path.Combine(output, "text-block-fixture.pdf");
        var saved = await _facade.SaveToDestinationAsync(_session!.SessionId, bytes => File.WriteAllBytesAsync(file, bytes));
        if (!saved.IsSuccess) throw new Exception(saved.Error!.Message);
        await OpenTextBlockSmokeAsync(await File.ReadAllBytesAsync(file));
        var reopened = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!;
        if (reopened.TextBlocks[0] is not { Text: "FACTURA 12", Runs.Count: 1 })
            throw new Exception("The save did not keep the retyped block as one run.");

        // Delete with the block selected removes it; one undo restores it.
        await OpenTextBlockSmokeAsync(HeadingPdf());
        SetContentEditMode(true);
        await BeginContentGestureAsync(_slots[0], 0, OnHeading, null!);
        await DeleteSelectedTextBlockAsync();
        await RequireBlocksAsync("after Delete on the selection", "Total");
        if (_selectedTextBlock is not null) throw new Exception("The deleted block stayed selected.");
        await ApplyHistoryAsync(true);
        await RequireBlocksAsync("after undoing Delete on the selection", "PRESUPUESTO N 3.725/1", "Total");

        // Delete text in a block editor does the same.
        await OpenContentEditorOnDoubleTapAsync(0, OnHeading);
        if (_pump.Box is not { Block: not null }) throw new Exception("A double-click did not open a block editor to delete from.");
        if (!DeleteTextButton.IsEnabled) throw new Exception("Delete text is not available in a block editor.");
        await DeleteOpenContentTextAsync();
        await RequireBlocksAsync("after the delete", "Total");
        await ApplyHistoryAsync(true);
        await RequireBlocksAsync("after undoing the delete", "PRESUPUESTO N 3.725/1", "Total");

        // Text typed in with Insert text: selected and deleted before a save.
        await OpenTextBlockSmokeAsync(HeadingPdf());
        SetContentEditMode(true);
        SetContentInsertMode(ContentInsertKind.Text);
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(100, 300), null!);
        _insertEditor!.Box.Text = "Chiclana de la Frontera";
        await CommitInsertEditorAsync();
        SetContentInsertMode(null);
        var typed = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextRuns.Single(run => run.Text == "Chiclana de la Frontera");
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(typed.Bounds.X + typed.Bounds.Width - 1, typed.Bounds.Y + typed.Bounds.Height / 2), null!);
        if (_selectedTextBlock?.First.Text != "Chiclana de la Frontera")
            throw new Exception("A click on the end of the typed text did not select it — its box is short.");
        await DeleteSelectedTextBlockAsync();
        await RequireBlocksAsync("after deleting typed text", "PRESUPUESTO N 3.725/1", "Total");

        // A one-run text retyped alone, then deleted with the selection.
        var total = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextBlocks.Single(block => block.Text == "Total").Bounds;
        var onTotal = new AnnotationPoint(total.X + 2, total.Y + total.Height / 2);
        await OpenContentEditorOnDoubleTapAsync(0, onTotal);
        _pump.Box!.Box.Text = "Total 2";
        await CommitContentEditorAsync();
        await BeginContentGestureAsync(_slots[0], 0, onTotal, null!);
        await DeleteSelectedTextBlockAsync();
        await RequireBlocksAsync("after deleting retyped text", "PRESUPUESTO N 3.725/1");

        // An image dragged twice before a save: the frame follows it each
        // time, the second drag is not refused, and one undo puts it back.
        await OpenTextBlockSmokeAsync(HeadingPdf());
        SetContentEditMode(true);
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(430, 520), null!);
        if (_contentImageDrag is null) throw new Exception("A press on the image did not start a drag.");
        await CompleteContentImageDragAsync(new AnnotationPoint(450, 500));
        RequireImage(new AnnotationRect(420, 480, 60, 40), "after the first drag");
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(450, 500), null!);
        if (_contentImageDrag is null) throw new Exception($"The moved image could not be dragged again. Status: {AnnotationStatus.Text}");
        await CompleteContentImageDragAsync(new AnnotationPoint(460, 510));
        RequireImage(new AnnotationRect(430, 490, 60, 40), "after the second drag");
        await ApplyHistoryAsync(true);
        var restored = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.Images.Single().Bounds;
        if (!Near(restored, new AnnotationRect(400, 500, 60, 40))) throw new Exception($"One undo left the image at {restored}.");

        if (Environment.GetEnvironmentVariable("VITELA_SMOKE_KEYS") == "1") await RealDeleteKeyAsync(output);
    }

    /// <summary>
    /// Opt-in: a real Delete keypress, sent from outside the process once
    /// <c>text-block-keys-ready</c> appears, with the Edit card collapsed —
    /// the state in which Delete used to do nothing.
    /// </summary>
    private async Task RealDeleteKeyAsync(string output)
    {
        await OpenTextBlockSmokeAsync(HeadingPdf());
        SetContentEditMode(true);
        SelectToolPage("Annotate");
        await BeginContentGestureAsync(_slots[0], 0, OnHeading, null!);
        if (_selectedTextBlock is null) throw new Exception("No block selected before the keypress.");
        File.WriteAllText(Path.Combine(output, "text-block-keys-ready"), Environment.ProcessId.ToString());
        for (var attempt = 0; ; attempt++)
        {
            if (attempt == 200) throw new Exception($"A real Delete keypress did not remove the selected block. Status: {AnnotationStatus.Text}");
            await Task.Delay(100);
            var texts = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextBlocks.Select(block => block.Text);
            if (texts.SequenceEqual(["Total"])) break;
        }

        // The same key on a selected image — moved first, so the deletion has
        // to fold into a pending edit.
        await BeginContentGestureAsync(_slots[0], 0, new AnnotationPoint(430, 520), null!);
        await CompleteContentImageDragAsync(new AnnotationPoint(450, 500));
        if (_selectedContentImage is null) throw new Exception("No image selected before the keypress.");
        File.WriteAllText(Path.Combine(output, "text-block-keys-ready-image"), Environment.ProcessId.ToString());
        for (var attempt = 0; attempt < 200; attempt++)
        {
            await Task.Delay(100);
            if ((await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.Images.Count == 0) return;
        }
        throw new Exception($"A real Delete keypress did not remove the selected image. Status: {AnnotationStatus.Text}");
    }

    private void RequireImage(AnnotationRect expected, string when)
    {
        if (_selectedContentImage is not { } selected || !Near(selected.Bounds, expected))
            throw new Exception($"The image frame {when} is {_selectedContentImage?.Bounds}, not {expected}. Status: {AnnotationStatus.Text}");
    }

    private static bool Near(AnnotationRect left, AnnotationRect right) =>
        Math.Abs(left.X - right.X) < 0.01 && Math.Abs(left.Y - right.Y) < 0.01
        && Math.Abs(left.Width - right.Width) < 0.01 && Math.Abs(left.Height - right.Height) < 0.01;

    private async Task<AnnotationRect> HeadingBoundsAsync() =>
        (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextBlocks
            .First(block => block.Text.StartsWith("PRESUPUES")).Bounds;

    private async Task RequireBlocksAsync(string when, params string[] expected)
    {
        var texts = (await _facade.PageContentAsync(_session!.SessionId, 0)).Value!.TextBlocks.Select(block => block.Text).ToArray();
        if (!texts.SequenceEqual(expected))
            throw new Exception($"Blocks {when} are [{string.Join(" | ", texts)}], not [{string.Join(" | ", expected)}]. Status: {AnnotationStatus.Text}");
    }

    private async Task OpenTextBlockSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Text block smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
    }

    /// <summary>The reference quote's heading as its generator wrote it, a column far to its right, and a picture.</summary>
    private static byte[] HeadingPdf()
    {
        const string content = "BT /F1 12 Tf 100 700 Td (PRESUPUES) Tj (TO N 3) Tj (.725/1) Tj 200 0 Td (Total) Tj ET "
            + "q 60 0 0 40 400 500 cm /Im1 Do Q";
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
