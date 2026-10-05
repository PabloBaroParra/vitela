using Microsoft.UI.Xaml;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _windowClosed;

    private void InitializeWindowLifecycle()
    {
        Activated += MainWindow_Activated;
        if (Content is FrameworkElement root) root.Loaded += WindowContent_Loaded;
        AppWindow.Closing += MainWindow_Closing;
        Closed += (_, _) =>
        {
            _windowClosed = true;
            AppWindow.Closing -= MainWindow_Closing;
            Activated -= MainWindow_Activated;
            if (Content is FrameworkElement root) root.Loaded -= WindowContent_Loaded;
            if (_xamlRoot is not null) _xamlRoot.Changed -= XamlRoot_Changed;
            _thumbnailGeneration++;
            _organizeDocumentsGeneration++;
            _organizeBlockDragToken = null;
            _organizing = false;
            StopOrganizeScroll();
            ClearOrganizePageState();
        };
    }

    private void MainWindow_Activated(object sender, WindowActivatedEventArgs e)
    {
        // Deactivation can arrive after Close, before a window ever acquired a root.
        if (e.WindowActivationState != WindowActivationState.Deactivated) AttachWindowXamlRoot();
    }

    private void WindowContent_Loaded(object sender, RoutedEventArgs e) => AttachWindowXamlRoot();

    private void AttachWindowXamlRoot()
    {
        // Activation may precede Loaded, or be denied while another application has focus.
        if (_windowClosed || _xamlRoot is not null || Content.XamlRoot is not { } xamlRoot) return;
        _xamlRoot = xamlRoot;
        _rasterizationScale = xamlRoot.RasterizationScale;
        xamlRoot.Changed += XamlRoot_Changed;
    }
}
