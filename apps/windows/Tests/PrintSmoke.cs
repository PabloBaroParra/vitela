using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

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
            await _window.PrintSmokeAsync();
            File.WriteAllText(Path.Combine(output, "print-smoke.log"),
                "PASS no-document/empty print states, full-flow busy and duplicate guards, real snapshot/preview bitmap, failed raster status, release/retry and unchanged session.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "print-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task PrintSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        for (var i = 0; i < 200 && Content.XamlRoot is null; i++) await Task.Delay(25);
        Check(Content.XamlRoot is not null, "Window not loaded");
        RefreshSessionCommands();
        Check(!PrintButton.IsEnabled, "Print enabled without document");
        PrintButton_Click(PrintButton, new RoutedEventArgs());
        Check(PrintStatus.Text == "Open a PDF before printing.", "No-document status");
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        var session = _session!;
        Check(PrintButton.IsEnabled, "Print unavailable with pages");
        _session = session with { PageCount = 0, State = Facade.DocumentSessionState.Empty };
        RefreshSessionCommands();
        Check(!PrintButton.IsEnabled, "Print enabled for empty document");
        PrintButton_Click(PrintButton, new RoutedEventArgs());
        Check(PrintStatus.Text == "The PDF has no pages to print.", "Empty status");
        _session = session;
        _printingDocument = true;
        SetBusy(false);
        Check(_isBusy && !PrintButton.IsEnabled && !OpenButton.IsEnabled && !SaveButton.IsEnabled,
            "Print ownership must survive unrelated busy settling");
        PrintStatus.Text = "guard sentinel";
        PrintButton_Click(PrintButton, new RoutedEventArgs());
        Check(PrintStatus.Text == "guard sentinel", "Duplicate print changed state");
        var prepared = await _facade.PreparePrintAsync(session.SessionId);
        Check(prepared.IsSuccess && prepared.Value == session.PageCount, "Real print snapshot");
        var job = new PrintJob(session.SessionId, session.DisplayName, (int)prepared.Value);
        _printJob = job;
        var first = GetPreviewBitmapAsync(job, 0);
        Check(ReferenceEquals(first, GetPreviewBitmapAsync(job, 0)), "Preview must share in-flight task");
        Check(await first is { PixelWidth: > 0, PixelHeight: > 0 }, "Real print preview raster");
        Check(await RenderPrintBitmapAsync(job, (int)session.PageCount, PrintDpi) is null,
            "Invalid page must fail safely");
        Check(PrintStatus.Text.Contains("It will be blank."), "Raster failure must be visible");
        FinishPrinting();
        Check(!_printingDocument && !_isBusy && _printJob is null && PrintButton.IsEnabled && OpenButton.IsEnabled,
            "Completion must release owner and restore controls");
        Check(_session == session, "Printing replaced the session");
        prepared = await _facade.PreparePrintAsync(session.SessionId);
        Check(prepared.IsSuccess, "Print must prepare again after release");
        _facade.ReleasePrint(session.SessionId);
    }
}
