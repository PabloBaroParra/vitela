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

    /// <param name="closing">Whether the window is closing, which has its own explanation.</param>
    /// <param name="detail">An explanation of its own — closing or reloading one tab — over the default ones.</param>
    private async Task<PendingEditDecision> AskPendingEditDecisionAsync(bool closing = false, string? detail = null)
    {
        var dialog = new ContentDialog
        {
            Title = I18n.T("open-unsaved-title"),
            Content = new TextBlock
            {
                Text = detail ?? (closing ? I18n.T("open-unsaved-detail-closing") : I18n.T("open-unsaved-detail-opening")),
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
        if (_confirmingClose || _dialogOpen || _shellPickingFile) return;
        if (_isBusy || _savingDocument || _protectingDocument || _compressingDocument
            || _exportingImages || _extractingPages || _splittingPages || _tabsOpening || _tabSwitching)
        {
            // Nothing happens on a refused close but this line: say so.
            AnnotationStatus.Text = I18n.T("tabs-close-held");
            return;
        }
        _confirmingClose = true;
        try
        {
            // Every tab with unsaved work is asked about in turn (MainWindow.TabClose.cs).
            if (!await ResolveUnsavedTabsForCloseAsync()) return;
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
