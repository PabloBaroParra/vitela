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
        var log = Path.Combine(output, "wheel-zoom-smoke.log");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.WheelZoomSmokeAsync();
            File.WriteAllText(log, "PASS native Ctrl+wheel in/out; fit-mode stepping; 10%-800% limits; normal/horizontal wheel; sidebar isolation; busy/modal/empty guards; unchanged history; no native ScrollViewer scaling.");
        }
        catch (Exception error) { File.WriteAllText(log, "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task WheelZoomSmokeAsync()
    {
        static void Check(bool ok, string message) { if (!ok) throw new Exception(message); }
        for (var i = 0; i < 200 && Content.XamlRoot is null; i++) await Task.Delay(25);
        Check(Content.XamlRoot is not null, "Window not loaded.");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        for (var i = 0; i < 200 && PageScroller.ViewportHeight == 0; i++) await Task.Delay(25);
        Check(PageScroller.ViewportHeight > 0, "Viewport not laid out.");
        var before = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        foreach (var (start, delta, expected) in new[] {
            (1.0, 120, 1.25), (1.0, -120, .75), (8.0, 120, 8.0), (.10, -120, .10) })
        {
            ApplyZoom(ZoomSetting.Custom(start));
            await SendWheelAsync(PageScroller, delta);
            Check(_zoom == ZoomSetting.Custom(expected), $"Wheel {delta}: expected {expected}, got {_zoom}.");
            Check(ZoomLevel.Text == $"{expected * 100:0}%", "Zoom readout disagrees.");
            Check(PageScroller.ZoomFactor == 1, "Native zoom bypassed PDF layout.");
        }
        ApplyZoom(ZoomSetting.FitWidth);
        var next = PageZoom.StepIn(CurrentFactor());
        await SendWheelAsync(PageScroller, 120);
        Check(_zoom == next, "Fit mode did not step from the effective zoom.");

        ApplyZoom(ZoomSetting.Custom(1));
        await SendWheelAsync(PageScroller, -120, control: false);
        Check(_zoom == ZoomSetting.Custom(1) && PageScroller.VerticalOffset > 0, "Normal wheel no longer scrolls.");
        await SendWheelAsync(PageScroller, 120, horizontal: true);
        Check(_zoom == ZoomSetting.Custom(1), "Horizontal wheel changed zoom.");
        await SendWheelAsync(PagesSidebar, 120);
        Check(_zoom == ZoomSetting.Custom(1), "Sidebar wheel changed document zoom.");

        _isBusy = true;
        await SendWheelAsync(PageScroller, 120);
        _isBusy = false;
        Check(_zoom == ZoomSetting.Custom(1), "Busy wheel changed zoom.");
        _dialogOpen = true;
        await SendWheelAsync(PageScroller, 120);
        _dialogOpen = false;
        Check(_zoom == ZoomSetting.Custom(1), "Modal wheel changed zoom.");
        var session = _session;
        _session = null;
        await SendWheelAsync(PageScroller, 120);
        _session = session;
        Check(_zoom == ZoomSetting.Custom(1), "Missing session changed zoom.");

        var after = (await _facade.AnnotationStateAsync(_session!.SessionId)).Value!;
        Check(before.CanUndo == after.CanUndo && before.CanRedo == after.CanRedo
            && before.Annotations.SequenceEqual(after.Annotations), "Wheel zoom changed document history.");
    }

    private async Task SendWheelAsync(FrameworkElement target, int delta, bool control = true, bool horizontal = false)
    {
        var point = target.TransformToVisual(null).TransformPoint(new global::Windows.Foundation.Point(40, 80));
        var scale = Content.XamlRoot.RasterizationScale;
        var screen = new WheelPoint { X = (int)(point.X * scale), Y = (int)(point.Y * scale) };
        if (!WheelClientToScreen(WinRT.Interop.WindowNative.GetWindowHandle(this), ref screen)
            || !WheelSetCursorPos(screen.X, screen.Y)) throw new Exception("Could not position native wheel input.");
        void Send(WheelInput input)
        {
            if (WheelSendInput(1, [input], Marshal.SizeOf<WheelInput>()) != 1)
                throw new Exception("Native input injection failed.");
        }
        if (control) Send(new WheelInput { Type = 1, Data = new WheelInputData { Key = new WheelKey { VirtualKey = 0x11 } } });
        try
        {
            await Task.Delay(60);
            Send(new WheelInput { Data = new WheelInputData { Mouse = new WheelMouse { Data = unchecked((uint)delta), Flags = horizontal ? 0x1000u : 0x0800u } } });
            await Task.Delay(250);
        }
        finally
        {
            if (control) Send(new WheelInput { Type = 1, Data = new WheelInputData { Key = new WheelKey { VirtualKey = 0x11, Flags = 2 } } });
        }
    }

    [StructLayout(LayoutKind.Sequential)] private struct WheelPoint { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)] private struct WheelInput { public uint Type; public WheelInputData Data; }
    [StructLayout(LayoutKind.Explicit)] private struct WheelInputData
    {
        [FieldOffset(0)] public WheelMouse Mouse;
        [FieldOffset(0)] public WheelKey Key;
    }
    [StructLayout(LayoutKind.Sequential)] private struct WheelMouse { public int X, Y; public uint Data, Flags, Time; public nuint Extra; }
    [StructLayout(LayoutKind.Sequential)] private struct WheelKey { public ushort VirtualKey, Scan; public uint Flags, Time; public nuint Extra; }
    [DllImport("user32.dll", EntryPoint = "SendInput", SetLastError = true)] private static extern uint WheelSendInput(uint count, WheelInput[] inputs, int size);
    [DllImport("user32.dll", EntryPoint = "ClientToScreen")] private static extern bool WheelClientToScreen(nint window, ref WheelPoint point);
    [DllImport("user32.dll", EntryPoint = "SetCursorPos")] private static extern bool WheelSetCursorPos(int x, int y);
}
