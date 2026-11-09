using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();

    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        var log = Path.Combine(output, "pan-smoke.log");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.PanSmokeAsync();
            File.WriteAllText(log, "PASS native Space+left drag pans both axes; open/closed hand cursors; Space released mid-drag stops panning and keeps the press from the tools; middle drag without Space; plain left press reaches text selection; focused text field keeps Space; empty-document guard; unchanged history.");
        }
        catch (Exception error) { File.WriteAllText(log, "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    private const ushort PanSpaceKey = 0x20;
    private const uint LeftDown = 0x0002, LeftUp = 0x0004, MiddleDown = 0x0020, MiddleUp = 0x0040;

    internal async Task PanSmokeAsync()
    {
        static void Check(bool ok, string message) { if (!ok) throw new Exception(message); }
        static bool Near(double a, double b) => Math.Abs(a - b) < 3;
        for (var i = 0; i < 200 && Content.XamlRoot is null; i++) await Task.Delay(25);
        Check(Content.XamlRoot is not null, "Window not loaded.");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        for (var i = 0; i < 200 && PageScroller.ViewportHeight == 0; i++) await Task.Delay(25);
        Check(PageScroller.ViewportHeight > 0, "Viewport not laid out.");
        var before = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;

        ApplyZoom(ZoomSetting.Custom(2.5));
        await Task.Delay(250);
        Check(PageScroller.ScrollableWidth > 300 && PageScroller.ScrollableHeight > 300, "Nothing to pan at 250%.");
        var center = new global::Windows.Foundation.Point(PageScroller.ViewportWidth / 2, PageScroller.ViewportHeight / 2);

        async Task Home()
        {
            PageScroller.ChangeView(200, 300, null, disableAnimation: true);
            await Task.Delay(200);
            await PanMoveAsync(center);
            _textSelection = null;
            _textDragActive = false;
        }
        (double H, double V) Offsets() => (PageScroller.HorizontalOffset, PageScroller.VerticalOffset);

        // Space + left drag: the document follows the pointer on both axes.
        await Home();
        await PanKeyAsync(down: true);
        Check(_hand.SpaceHeld && _hand.Cursor == PanCursor.Grab, "Space over the document did not arm the open hand.");
        await PanKeyAsync(down: true); // autorepeat
        Check(_hand.SpaceHeld, "Autorepeat disarmed the hand.");
        var (h0, v0) = Offsets();
        await PanButtonAsync(LeftDown);
        Check(_hand.Dragging && _hand.Cursor == PanCursor.Grabbing, "Space + left press was not claimed.");
        await PanMoveAsync(new(center.X - 60, center.Y - 80));
        var (h1, v1) = Offsets();
        Check(Near(h1, h0 + 60) && Near(v1, v0 + 80), $"Pan moved to ({h1}, {v1}), expected ({h0 + 60}, {v0 + 80}).");
        await PanButtonAsync(LeftUp);
        Check(!_hand.Dragging && _hand.Cursor == PanCursor.Grab, "Button up did not return to the open hand.");
        Check(_textSelection is null, "A claimed pan press reached text selection.");
        await PanKeyAsync(down: false);
        Check(!_hand.SpaceHeld && _hand.Cursor == PanCursor.None, "Space up did not restore the cursor.");

        // Space released mid-drag: no more panning, press still claimed.
        await Home();
        await PanKeyAsync(down: true);
        await PanButtonAsync(LeftDown);
        await PanMoveAsync(new(center.X + 40, center.Y + 40));
        await PanKeyAsync(down: false);
        var stopped = Offsets();
        await PanMoveAsync(new(center.X + 140, center.Y + 140));
        Check(Offsets() == stopped, "Pan kept moving after Space came up.");
        Check(_hand.Dragging && _hand.Cursor == PanCursor.None, "Press was released before the button came up.");
        await PanButtonAsync(LeftUp);
        Check(!_hand.Dragging && _textSelection is null, "The rest of a stopped pan press leaked to the tools.");

        // Middle drag needs no Space.
        await Home();
        (h0, v0) = Offsets();
        await PanButtonAsync(MiddleDown);
        Check(_hand.Dragging && _hand.Cursor == PanCursor.Grabbing, "Middle press was not claimed.");
        await PanMoveAsync(new(center.X + 50, center.Y - 30));
        (h1, v1) = Offsets();
        Check(Near(h1, h0 - 50) && Near(v1, v0 + 30), $"Middle pan moved to ({h1}, {v1}), expected ({h0 - 50}, {v0 + 30}).");
        await PanButtonAsync(MiddleUp);
        Check(!_hand.Dragging && _hand.Cursor == PanCursor.None, "Middle release did not end the pan.");

        // A plain left press still reaches the page tools.
        await Home();
        var plain = Offsets();
        await PanButtonAsync(LeftDown);
        Check(!_hand.Dragging, "A plain left press was claimed by the pan.");
        await PanMoveAsync(new(center.X + 80, center.Y + 30));
        await PanButtonAsync(LeftUp);
        Check(Offsets() == plain, "A plain left drag panned.");
        Check(_textSelection is not null, "A plain left drag did not reach text selection.");
        _textSelection = null;
        RedrawSelection();

        // A focused text field keeps Space.
        Check(SearchBox.Focus(FocusState.Programmatic), "Could not focus the search box.");
        await Task.Delay(100);
        await PanKeyAsync(down: true);
        Check(!_hand.SpaceHeld, "Space was taken from a focused text field.");
        await PanKeyAsync(down: false);
        Check(SearchBox.Text == " ", $"The search box did not receive Space ('{SearchBox.Text}').");
        SearchBox.Text = "";
        PageScroller.Focus(FocusState.Programmatic);
        await Task.Delay(100);

        // Nothing to pan without a document.
        var session = _session;
        _session = null;
        await PanKeyAsync(down: true);
        Check(!_hand.SpaceHeld, "Space armed the hand without a document.");
        await PanButtonAsync(MiddleDown);
        Check(!_hand.Dragging, "Middle press claimed without a document.");
        await PanButtonAsync(MiddleUp);
        await PanKeyAsync(down: false);
        _session = session;

        var after = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        Check(before.CanUndo == after.CanUndo && before.CanRedo == after.CanRedo
            && before.Annotations.SequenceEqual(after.Annotations), "Panning changed document history.");
    }

    private async Task PanMoveAsync(global::Windows.Foundation.Point target)
    {
        var point = PageScroller.TransformToVisual(null).TransformPoint(target);
        var scale = Content.XamlRoot.RasterizationScale;
        var screen = new PanPoint { X = (int)Math.Round(point.X * scale), Y = (int)Math.Round(point.Y * scale) };
        if (!PanClientToScreen(WinRT.Interop.WindowNative.GetWindowHandle(this), ref screen))
            throw new Exception("Could not position the native pointer.");
        // SetCursorPos alone raised no PointerMoved in WinUI here; a small
        // relative nudge there and back is real mouse input.
        if (!PanSetCursorPos(screen.X, screen.Y)) throw new Exception("Could not position the native pointer.");
        PanSend(new PanInput { Data = new PanInputData { Mouse = new PanMouse { X = 8, Flags = 0x0001 } } });
        PanSend(new PanInput { Data = new PanInputData { Mouse = new PanMouse { X = -8, Flags = 0x0001 } } });
        await Task.Delay(150);
    }

    private async Task PanButtonAsync(uint flags)
    {
        PanSend(new PanInput { Data = new PanInputData { Mouse = new PanMouse { Flags = flags } } });
        await Task.Delay(150);
    }

    private async Task PanKeyAsync(bool down)
    {
        PanSend(new PanInput { Type = 1, Data = new PanInputData { Key = new PanKey { VirtualKey = PanSpaceKey, Flags = down ? 0u : 2u } } });
        await Task.Delay(150);
    }

    private static void PanSend(PanInput input)
    {
        if (PanSendInput(1, [input], Marshal.SizeOf<PanInput>()) != 1) throw new Exception("Native input injection failed.");
    }

    [StructLayout(LayoutKind.Sequential)] private struct PanPoint { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] private struct PanInput { public uint Type; public PanInputData Data; }
    [StructLayout(LayoutKind.Explicit)] private struct PanInputData
    {
        [FieldOffset(0)] public PanMouse Mouse;
        [FieldOffset(0)] public PanKey Key;
    }
    [StructLayout(LayoutKind.Sequential)] private struct PanMouse { public int X, Y; public uint Data, Flags, Time; public nuint Extra; }
    [StructLayout(LayoutKind.Sequential)] private struct PanKey { public ushort VirtualKey, Scan; public uint Flags, Time; public nuint Extra; }
    [DllImport("user32.dll", EntryPoint = "SendInput", SetLastError = true)] private static extern uint PanSendInput(uint count, PanInput[] inputs, int size);
    [DllImport("user32.dll", EntryPoint = "ClientToScreen")] private static extern bool PanClientToScreen(nint window, ref PanPoint point);
    [DllImport("user32.dll", EntryPoint = "SetCursorPos")] private static extern bool PanSetCursorPos(int x, int y);
}
