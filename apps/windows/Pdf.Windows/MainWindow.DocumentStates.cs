using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Empty/error presentation and availability of document-wide commands.</summary>
public sealed partial class MainWindow
{
    private void ShowEmpty(string message)
    {
        CloseOrganizeView();
        EmptyStateMessage.Text = message;
        EmptyState.Visibility = Visibility.Visible;
        ErrorState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
        PageCounter.Text = "";
        PageNavigationList.ItemsSource = null;
        ZoomLevel.Text = "";
        RefreshSessionCommands();
    }

    private void ShowError(UserSafeError error)
    {
        CloseOrganizeView();
        ErrorState.Text = $"{error.Message} Reference: {error.CorrelationId}";
        ErrorState.Visibility = Visibility.Visible;
        EmptyState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
        RefreshSessionCommands();
    }

    private void SetBusy(bool isBusy)
    {
        isBusy |= _printingDocument;
        _isBusy = isBusy;
        BusyIndicator.IsActive = isBusy;
        BusyIndicator.Visibility = isBusy ? Visibility.Visible : Visibility.Collapsed;
        OpenButton.IsEnabled = !isBusy;
        OpenSampleButton.IsEnabled = !isBusy;
        UpdateMatchButtons();
        RefreshSessionCommands();
        // Busy is a gate, not a replacement for the current permission snapshot.
        UpdateAnnotationControls(_annotationState);
    }

    private void RefreshSessionCommands()
    {
        var hasPages = !_isBusy && _session is { PageCount: > 0 };
        ZoomInButton.IsEnabled = hasPages;
        ZoomOutButton.IsEnabled = hasPages;
        FitWidthButton.IsEnabled = hasPages;
        FitPageButton.IsEnabled = hasPages;
        PrintButton.IsEnabled = hasPages;
        UpdateSearchControls();
        SaveButton.IsEnabled = hasPages;
        ProtectButton.IsEnabled = !_isBusy && _session?.ContentEditingAllowed == true;
        // Core owns compression and output permission refusals, not the shell.
        CompressButton.IsEnabled = !_isBusy && _session is not null;
        ExportImagesButton.IsEnabled = hasPages;
        ExtractPagesButton.IsEnabled = hasPages;
        SplitPagesButton.IsEnabled = !_isBusy && _session is { PageCount: > 1 };
        OrganizeButton.IsEnabled = hasPages;
        UpdateMetadataControls();
        FormFieldsScroller.IsEnabled = !_isBusy && _session is not null;
        UpdatePageNavigationControls();
    }
}
