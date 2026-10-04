using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

/// <summary>Dedicated Organize screen chrome; commands and their gates remain with their existing owners.</summary>
public sealed partial class MainWindow
{
    private bool _organizeHeaderInitialized;

    private void OrganizePanel_Loaded(object sender, RoutedEventArgs e)
    {
        if (!_organizeHeaderInitialized)
        {
            _organizeHeaderInitialized = true;
            InitializeOrganizeScroll();
            foreach (var source in new[] { UndoButton, RedoButton, ExtractPagesButton, SplitPagesButton, SaveButton, OpenButton })
                source.RegisterPropertyChangedCallback(Control.IsEnabledProperty, (_, _) => UpdateOrganizeHeader());
        }
        UpdateOrganizeHeader();
    }

    private void UpdateOrganizeHeader()
    {
        var ready = _organizing && !_isBusy && !_organizeBusy && _session is not null;
        OrganizeUndoButton.IsEnabled = ready && UndoButton.IsEnabled;
        OrganizeRedoButton.IsEnabled = ready && RedoButton.IsEnabled;
        OrganizeExtractButton.IsEnabled = ready && ExtractPagesButton.IsEnabled;
        OrganizeSplitButton.IsEnabled = ready && SplitPagesButton.IsEnabled;
        OrganizeSaveButton.IsEnabled = ready && SaveButton.IsEnabled;
        OrganizeReturnButton.IsEnabled = ready;
        OrganizeAddPdfsButton.IsEnabled = ready;
        InsertBlankPageButton.IsEnabled = ready;
        InsertLandscapePageButton.IsEnabled = ready;
        OrganizeGrid.IsEnabled = ready;
        OrganizeDocumentsList.IsHitTestVisible = ready;
        OrganizeDocumentsToggle.IsEnabled = ready;
        OrganizePagesToggle.IsEnabled = ready;
        SetOrganizeDocumentActionsEnabled(ready);
        UpdateOrganizePageActions();
    }

    private void OrganizeReturnButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _organizeBusy || _dialogOpen) return;
        LeaveOrganizeView();
        MarkRailDestination("Annotate");
    }

    private async void OrganizeUndoButton_Click(object sender, RoutedEventArgs e) => await RunOrganizeHistoryAsync(undo: true);

    private async void OrganizeRedoButton_Click(object sender, RoutedEventArgs e) => await RunOrganizeHistoryAsync(undo: false);

    private async Task RunOrganizeHistoryAsync(bool undo)
    {
        if (!_organizing || _session is null || _isBusy || _organizeBusy || _dialogOpen) return;
        if (!(undo ? OrganizeUndoButton.IsEnabled : OrganizeRedoButton.IsEnabled)) return;
        _organizeBusy = true;
        SetBusy(true);
        UpdateOrganizeHeader();
        try { await ApplyHistoryAndMetadataAsync(undo); }
        finally
        {
            _organizeBusy = false;
            SetBusy(false);
            UpdateAnnotationControls(_annotationState);
            UpdateOrganizeHeader();
        }
    }
}
