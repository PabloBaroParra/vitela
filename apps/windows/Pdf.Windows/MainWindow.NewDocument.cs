using Microsoft.UI.Xaml.Input;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private async void NewDocument_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        await CreateNewDocumentAsync();
    }

    /// <summary>
    /// Home and Ctrl+N share the existing Save/Discard/Cancel guard. The facade
    /// creates a document with its first A4 page already present: recording an
    /// InsertBlankPage on a zero-page document would not create a render handle
    /// and would introduce an unwanted pending edit before the user did anything.
    /// </summary>
    private async Task CreateNewDocumentAsync()
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        SetBusy(true);
        if (!await PrepareDocumentLifecycleAsync())
        {
            SetBusy(false);
            RestoreAnnotationControls();
            return;
        }
        var result = await _facade.CreateBlankAsync();
        SetBusy(false);

        if (!result.IsSuccess && result.Error!.RequiresPendingEditDecision)
        {
            switch (await AskPendingEditDecisionAsync())
            {
                case PendingEditDecision.Cancel:
                    RestoreAnnotationControls();
                    return;
                case PendingEditDecision.Save when !await SaveToPickedFileAsync():
                    // A cancelled picker or failed write must never discard the work.
                    RestoreAnnotationControls();
                    return;
                case PendingEditDecision.Save:
                    result = await _facade.CreateBlankAsync();
                    break;
                case PendingEditDecision.Discard:
                    result = await _facade.CreateBlankAsync(discardPendingEdits: true);
                    break;
            }
        }

        if (!result.IsSuccess)
        {
            ReportFailedOpen(result.Error!);
            return;
        }
        ShowOpenedDocument(result.Value!);
    }
}
