using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void NewDocument_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        await CreateNewDocumentAsync();
    }

    /// <summary>
    /// Home and Ctrl+N open a blank document in a tab of its own, so the
    /// document on screen — saved or not — is parked rather than replaced and
    /// there is nothing to ask about. The facade creates it with its first A4
    /// page already present: recording an InsertBlankPage on a zero-page
    /// document would not create a render handle and would introduce an
    /// unwanted pending edit before the user did anything.
    /// </summary>
    private async Task CreateNewDocumentAsync()
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        await OpenInTabsAsync([new TabOpenRequest(I18n.T("editor-untitled"), null, CreateBlankAsync)]);
    }

    private async Task<OperationResult<DocumentSession>?> CreateBlankAsync(PdfDocumentFacade facade, bool discardPendingEdits)
    {
        SetBusy(true);
        try
        {
            return await facade.CreateBlankAsync(discardPendingEdits);
        }
        finally
        {
            SetBusy(false);
        }
    }
}
