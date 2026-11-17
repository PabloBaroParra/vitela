using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Windowing;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _confirmingClose;
    private bool _approvedClose;
    private enum PendingEditDecision { Cancel, Save, Discard }

    private async Task<bool> PrepareDocumentLifecycleAsync()
    {
        // History settling intentionally closes without sending the paused text.
        // Lifecycle writes/replacements must commit it instead, and keep a refused box.
        await CommitContentEditorAsync();
        await _formFillTail;
        if (_pump.Box is null) return true;
        AnnotationStatus.Text = I18n.T("open-resolve-inline-edit");
        return false;
    }

    private async Task<PendingEditDecision> AskPendingEditDecisionAsync(bool closing = false)
    {
        var dialog = new ContentDialog
        {
            Title = I18n.T("open-unsaved-title"),
            Content = new TextBlock
            {
                Text = closing ? I18n.T("open-unsaved-detail-closing") : I18n.T("open-unsaved-detail-opening"),
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = I18n.T("open-unsaved-save"),
            SecondaryButtonText = I18n.T("open-unsaved-discard"),
            CloseButtonText = I18n.T("open-unsaved-cancel"),
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        var decision = await ShowModalAsync(dialog) switch
        {
            ContentDialogResult.Primary => PendingEditDecision.Save,
            ContentDialogResult.Secondary => PendingEditDecision.Discard,
            _ => PendingEditDecision.Cancel,
        };
        if (decision == PendingEditDecision.Cancel) AnnotationStatus.Text = I18n.T("open-unsaved-kept");
        return decision;
    }

    private async void MainWindow_Closing(AppWindow sender, AppWindowClosingEventArgs args)
    {
        // Cancel synchronously: an async confirmation cannot defer the native event.
        if (_approvedClose) return;
        args.Cancel = true;
        if (_confirmingClose || _dialogOpen || _shellPickingFile || _isBusy
            || _savingDocument || _protectingDocument || _compressingDocument
            || _exportingImages || _extractingPages || _splittingPages) return;
        _confirmingClose = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return;
            var sessionId = _session?.SessionId;
            var pending = sessionId is null ? null : await _facade.HasUnsavedChangesAsync(sessionId);
            SetBusy(false);
            RestoreAnnotationControls();
            if (pending is { IsSuccess: false })
            {
                AnnotationStatus.Text = pending.Error!.Message;
                return;
            }
            if (pending?.Value == true)
            {
                var decision = await AskPendingEditDecisionAsync(closing: true);
                if (decision == PendingEditDecision.Cancel)
                {
                    return;
                }
                if (decision == PendingEditDecision.Save && !await SaveToPickedFileAsync()) return;
            }
            if (_session?.SessionId != sessionId) return;
            // Close destroys this window; no second system-close confirmation is needed.
            _approvedClose = true;
            Close();
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.OpenReadFailure(error).Error!.Message;
        }
        finally
        {
            _confirmingClose = false;
            if (!_windowClosed)
            {
                SetBusy(false);
                RestoreAnnotationControls();
            }
        }
    }
}
