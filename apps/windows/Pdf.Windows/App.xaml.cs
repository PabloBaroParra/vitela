using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Recovery;

namespace Pdf.Windows;

public partial class App : Application
{
    private Window? _window;

    /// <remarks>
    /// The PDFium override is set here, not in <see cref="OnLaunched"/>: it has
    /// to be in place before the first core call, and the constructor is the
    /// earliest managed code this app owns. See <see cref="BundledPdfium"/> for
    /// why a shipped build cannot rely on the core's other resolution steps.
    /// </remarks>
    public App()
    {
        BundledPdfium.PointCoreAtBundledLibrary();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var window = new MainWindow();
        _window = window;
        window.Activate();
        // Only the real startup autosaves; the smoke harnesses replace this App.
        window.EnableCrashRecovery(new RecoveryDirectory(RecoveryDirectory.DefaultRoot));
        // Files named on the command line — Explorer's "Open with" on one or
        // several PDFs — open as document tabs once the window is ready.
        var files = Environment.GetCommandLineArgs().Skip(1).Where(File.Exists).ToList();
        if (files.Count > 0) window.OpenAtStartup(files);
    }
}
