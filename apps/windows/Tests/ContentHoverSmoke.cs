using System.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation.Peers;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

// Opt-in native runtime harness for the Edit content hover (Linux PR #567
// port): the same replacement-entry-point pattern as the other smoke tests.
// A client box (a stroked frame with two lines inside), a logo beside it and
// a footer, driven through the real canvas code and the real core.
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
            await _window.ContentHoverSmokeAsync();
            File.WriteAllText(Path.Combine(output, "content-hover-smoke.log"),
                "PASS the logo under the pointer is outlined two screen pixels out, on an upright and a quarter-turned page; "
                + "its outer border band keeps the hover and shows the move cursor; a press there moves the image, and the "
                + "hover is found again round where it landed without the pointer moving; the footer moves by its outer "
                + "band; a click on a block's outer band selects it; the quick-action bar shows beside the selected block "
                + "with Edit text and without Replace, its Delete removes the block, and both bar and hover stand down "
                + "under the inline editor.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "content-hover-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task ContentHoverSmokeAsync()
    {
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        if (Content.XamlRoot is null) throw new Exception("Window did not load.");

        foreach (var rotate in new[] { 0, 90 })
        {
            await OpenHoverSmokeAsync(HoverTablePdf(rotate));
            var content = await HoverContentAsync();
            var logo = content.Images.Single();
            Hover(new AnnotationPoint(logo.Bounds.X + logo.Bounds.Width / 2, logo.Bounds.Y + logo.Bounds.Height / 2));
            RequireHover($"over the logo (rotate {rotate})", ContentHoverKind.Image, ContentHoverZone.Inside);
            RequireOutlineAround(logo.Bounds, $"rotate {rotate}");
        }

        await OpenHoverSmokeAsync(HoverTablePdf(0));
        var scale = _slots[0].Scale;
        var outlineOut = ContentHover.OutlinePadPx / scale;

        // The logo's outer band: off its box, still its border.
        var image = (await HoverContentAsync()).Images.Single();
        Hover(new AnnotationPoint(image.Bounds.X + 5, image.Bounds.Y + 20));
        var band = new AnnotationPoint(image.Bounds.X - outlineOut, image.Bounds.Y + 20);
        Hover(band);
        RequireHover("on the logo's outer band", ContentHoverKind.Image, ContentHoverZone.Border);
        if (_moveCursorSlot is null) throw new Exception("The border band did not show the move cursor.");

        // A press there moves the whole image; the hover comes back round it.
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, band, null!);
        if (_contentImageDrag is null) throw new Exception($"A press on the logo's border did not grab it. Status: {AnnotationStatus.Text}");
        var dropped = new AnnotationPoint(band.X, band.Y - 100);
        Hover(dropped);
        if (_contentHoverOutline.Parent is not null) throw new Exception("The hover outline showed during the drag.");
        _contentPointerDown = false;
        await CompleteContentImageDragAsync(dropped);
        var moved = (await HoverContentAsync()).Images.Single();
        if (Math.Abs(moved.Bounds.Y - (image.Bounds.Y - 100)) > 0.05 || Math.Abs(moved.Bounds.X - image.Bounds.X) > 0.05)
            throw new Exception($"The logo is at {moved.Bounds}, not 100pt below {image.Bounds}. Status: {AnnotationStatus.Text}");
        await WaitForAsync(() => _contentHover is { Member.Kind: ContentHoverKind.Image }, "the hover to come back round the moved logo");
        RequireHover("after the border move", ContentHoverKind.Image, ContentHoverZone.Border);
        RequireOutlineAround(moved.Bounds, "after the border move");

        // The footer moves by its outer band as a whole block.
        var footer = (await HoverContentAsync()).TextBlocks.Single(block => block.Text == "Pag 1");
        var footerBox = DrawnBounds(footer);
        Hover(new AnnotationPoint(footerBox.X + 2, footerBox.Y + footerBox.Height / 2));
        var footerBand = new AnnotationPoint(footerBox.X - outlineOut, footerBox.Y + footerBox.Height / 2);
        Hover(footerBand);
        RequireHover("on the footer's outer band", ContentHoverKind.Text, ContentHoverZone.Border);
        _contentPointerDown = true;
        await BeginContentGestureAsync(_slots[0], 0, footerBand, null!);
        if (_textBlockDrag is null) throw new Exception($"A press on the footer's border did not grab it. Status: {AnnotationStatus.Text}");
        _contentPointerDown = false;
        await CompleteTextBlockGestureAsync(new AnnotationPoint(footerBand.X + 50, footerBand.Y), dragging: true);
        var footerNow = (await HoverContentAsync()).TextBlocks.Single(block => block.Text == "Pag 1").Bounds;
        if (Math.Abs(footerNow.X - (footer.Bounds.X + 50)) > 0.5)
            throw new Exception($"The footer is at {footerNow}, not 50pt right of {footer.Bounds}. Status: {AnnotationStatus.Text}");

        // A click on a block's outer band selects it rather than starting a marquee.
        var cliente = (await HoverContentAsync()).TextBlocks.Single(block => block.Text == "Cliente");
        var clienteBox = DrawnBounds(cliente);
        // Its right edge: the left one sits close enough to the frame's own
        // border for the frame to win the hit-test at a low zoom.
        var clienteRight = clienteBox.X + clienteBox.Width;
        Hover(new AnnotationPoint(clienteRight - 2, clienteBox.Y + clienteBox.Height / 2));
        var clienteBand = new AnnotationPoint(clienteRight + outlineOut, clienteBox.Y + clienteBox.Height / 2);
        Hover(clienteBand);
        RequireHover("on Cliente's outer band", ContentHoverKind.Text, ContentHoverZone.Border);
        _contentPointerDown = false;
        await BeginContentGestureAsync(_slots[0], 0, clienteBand, null!);
        if (_selectedTextBlock?.Text != "Cliente" || _selectionGesture is not null)
            throw new Exception($"A click on Cliente's border selected {_selectedTextBlock?.Text ?? "nothing"}. Status: {AnnotationStatus.Text}");

        // The quick-action bar beside the selected block.
        // Waits out the refresh the selection scheduled: the bar was already
        // up for the image and the footer.
        await WaitForAsync(() => QuickActionsBar.Visibility == Visibility.Visible && _quickRetype.Visibility == Visibility.Visible,
            "the quick-action bar to show for the block");
        if (_quickRetype.Visibility != Visibility.Visible || !_quickRetype.IsEnabled || _quickReplace.Visibility != Visibility.Collapsed
            || !_quickCut.IsEnabled || !_quickCopy.IsEnabled || !_quickDelete.IsEnabled)
            throw new Exception($"The bar for a block offers retype {_quickRetype.Visibility}/{_quickRetype.IsEnabled}, replace "
                + $"{_quickReplace.Visibility}, cut {_quickCut.IsEnabled}, copy {_quickCopy.IsEnabled}, delete {_quickDelete.IsEnabled}.");
        var barLeft = Canvas.GetLeft(QuickActionsBar);
        var placed = _facade.PlaceRect(clienteBox, PlacementOf(_slots[0], 0));
        var anchor = _slots[0].Content.TransformToVisual(QuickActionsLayer).TransformPoint(new global::Windows.Foundation.Point(placed.Left, placed.Top));
        if (Math.Abs(barLeft - Math.Round(anchor.X + placed.Width + ContentQuickActions.GapPx)) > 1)
            throw new Exception($"The bar is at x={barLeft}, not beside the block's right edge at {anchor.X + placed.Width}.");

        // Its Delete removes the block — the Delete key's own handler.
        new ButtonAutomationPeer(_quickDelete).Invoke();
        await WaitForAsync(() => !_isBusy && _pageContent.TryGetValue(0, out var state) && state.Content is { } now
            && now.TextBlocks.All(block => block.Text != "Cliente"), "the bar's Delete to remove Cliente");
        await WaitForAsync(() => QuickActionsBar.Visibility == Visibility.Collapsed, "the bar to hide once nothing is selected");

        // Retype from the bar: the editor opens, and bar and hover stand down.
        var domingo = (await HoverContentAsync()).TextBlocks.Single(block => block.Text == "Domingo");
        SelectTextBlock(domingo);
        Hover(new AnnotationPoint(domingo.Bounds.X + 2, domingo.Bounds.Y + domingo.Bounds.Height / 2));
        await WaitForAsync(() => QuickActionsBar.Visibility == Visibility.Visible, "the bar beside Domingo");
        new ButtonAutomationPeer(_quickRetype).Invoke();
        await WaitForAsync(() => _pump.Box is not null, "the bar's Edit text to open the editor");
        await WaitForAsync(() => QuickActionsBar.Visibility == Visibility.Collapsed, "the bar to hide under the editor");
        Hover(new AnnotationPoint(domingo.Bounds.X + 2, domingo.Bounds.Y + domingo.Bounds.Height / 2));
        if (_contentHover is not null || _contentHoverOutline.Parent is not null) throw new Exception("The hover showed under the editor.");
        CancelContentEditor();
    }

    private void Hover(AnnotationPoint point) => TrackContentHover(_slots[0], 0, point);

    private async Task<PageContent> HoverContentAsync() =>
        await EnsurePageContentAsync(0) ?? throw new Exception($"Page 1 has no content. Status: {AnnotationStatus.Text}");

    private void RequireHover(string when, ContentHoverKind kind, ContentHoverZone zone)
    {
        if (_contentHover is not { } hover || hover.Member.Kind != kind || hover.Zone != zone)
            throw new Exception($"The hover {when} is {_contentHover?.Member.Kind}/{_contentHover?.Zone}, not {kind}/{zone}.");
    }

    private void RequireOutlineAround(AnnotationRect bounds, string when)
    {
        var slot = _slots[0];
        if (!ReferenceEquals(_contentHoverOutline.Parent, slot.Content)) throw new Exception($"No hover outline on the page {when}.");
        var expected = _facade.PlaceRect(ContentHover.OutlineRect(bounds, slot.Scale), PlacementOf(slot, 0));
        var item = _facade.PlaceRect(bounds, PlacementOf(slot, 0));
        var (left, top) = (Canvas.GetLeft(_contentHoverOutline), Canvas.GetTop(_contentHoverOutline));
        if (Math.Abs(left - expected.Left) > 0.01 || Math.Abs(top - expected.Top) > 0.01
            || Math.Abs(_contentHoverOutline.Width - expected.Width) > 0.01 || Math.Abs(_contentHoverOutline.Height - expected.Height) > 0.01)
            throw new Exception($"The outline {when} is at ({left}, {top}, {_contentHoverOutline.Width}, {_contentHoverOutline.Height}), not {expected}.");
        // Two screen pixels outside the item's own box, on every side.
        if (Math.Abs(item.Left - left - ContentHover.OutlinePadPx) > 0.01 || Math.Abs(_contentHoverOutline.Width - item.Width - 2 * ContentHover.OutlinePadPx) > 0.01)
            throw new Exception($"The outline {when} is not {ContentHover.OutlinePadPx}px outside the item at {item}.");
    }

    private static async Task WaitForAsync(Func<bool> condition, string what)
    {
        for (var attempt = 0; attempt < 100; attempt++)
        {
            if (condition()) return;
            await Task.Delay(50);
        }
        throw new Exception($"Timed out waiting for {what}.");
    }

    private async Task OpenHoverSmokeAsync(byte[] bytes)
    {
        var opened = await _facade.OpenAsync(new DocumentSource("Content hover smoke", bytes), discardPendingEdits: true);
        if (!opened.IsSuccess) throw new Exception(opened.Error!.Message);
        ShowOpenedDocument(opened.Value!);
        await RefreshAnnotationStateAsync();
        for (var attempt = 0; _slots.Count == 0 && attempt < 100; attempt++) await Task.Delay(50);
        if (_slots.Count == 0) throw new Exception("Pages were not laid out.");
        for (var attempt = 0; _slots[0].Scale <= 0 && attempt < 100; attempt++) await Task.Delay(50);
        SetContentEditMode(true);
    }

    /// <summary>The selection smoke's client box, logo and footer, optionally quarter-turned.</summary>
    private static byte[] HoverTablePdf(int rotate)
    {
        const string content = "0.5 w 300 600 200 80 re S "
            + "BT /F1 12 Tf 310 650 Td (Cliente) Tj 0 -20 Td (Domingo) Tj ET "
            + "q 60 0 0 40 100 620 cm /Im1 Do Q "
            + "BT /F1 10 Tf 300 40 Td (Pag 1) Tj ET";
        string[] objects =
        [
            "<< /Type /Catalog /Pages 2 0 R >>",
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
            $"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Rotate {rotate} /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> /XObject << /Im1 6 0 R >> >> >>",
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
