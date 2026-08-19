using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

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
        _window = new MainWindow();
        _window.Activate();
    }
}
