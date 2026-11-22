using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Localization;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Window-local view navigation. Switching Home never replaces a session.
/// The mode bar (MainWindow.ModeBar.cs) follows these views; nothing here tells it.
/// </summary>
public sealed partial class MainWindow
{
    private bool _shellInitialized;
    private bool _shellPickingFile;

    private void ShellRoot_Loaded(object sender, RoutedEventArgs e)
    {
        if (_shellInitialized) return;
        _shellInitialized = true;
        BuildEditorToolbar();
        BuildToolPages();
        BuildModeBar();
        BuildHomeTools();
        InitializeRecents();
        SetBusy(false);
        // Mirror the existing operation guard rather than introduce another busy owner.
        OpenButton.RegisterPropertyChangedCallback(Control.IsEnabledProperty, (_, _) =>
            SetNavigationEnabled(OpenButton.IsEnabled && !_shellPickingFile));
        OpenStartupPaths();
    }

    private void SetNavigationEnabled(bool enabled)
    {
        SetModeBarEnabled(enabled);
        HomeView.IsEnabled = enabled;
    }

    private void ShowHomeView()
    {
        EditorView.Visibility = Visibility.Collapsed;
        OrganizePanel.Visibility = Visibility.Collapsed;
        HomeView.Visibility = Visibility.Visible;
        ReturnToDocumentButton.Visibility = _session is null ? Visibility.Collapsed : Visibility.Visible;
        UpdateTabStripVisibility();
        _ = RefreshRecentFilesAsync();
    }

    private void ShowEditorView()
    {
        _recentGeneration++;
        HomeView.Visibility = Visibility.Collapsed;
        EditorView.Visibility = _organizing ? Visibility.Collapsed : Visibility.Visible;
        OrganizePanel.Visibility = _organizing ? Visibility.Visible : Visibility.Collapsed;
        UpdateTabStripVisibility();
    }

    private void ReturnToDocument_Click(object sender, RoutedEventArgs e)
    {
        ShowEditorView();
    }

    private async void ShellOpen_Click(object sender, RoutedEventArgs e) => await PickShellDocumentAsync();

    private async Task<bool> PickShellDocumentAsync()
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return false;
        _shellPickingFile = true;
        SetNavigationEnabled(false);
        try
        {
            var picker = new FileOpenPicker();
            picker.FileTypeFilter.Add(".pdf");
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            // Several files open as several tabs: the first in front, the rest behind it.
            var files = await picker.PickMultipleFilesAsync();
            if (files is null || files.Count == 0) return false;
            var previous = _session;
            await OpenFilesInTabsAsync(files.ToList());
            return _session is not null && !ReferenceEquals(previous, _session);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.OpenReadFailure(error).Error!.Message;
            return false;
        }
        finally
        {
            _shellPickingFile = false;
            SetNavigationEnabled(!_isBusy);
        }
    }

    private async Task NavigateToToolAsync(string destination, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        // Windows keeps no pending tool: the tool below is applied in this same
        // continuation, and only when the pick installed a new session. A
        // dismissed picker, a failed open or an abandoned password prompt ends
        // here, so no later open can inherit it (GTK abandon_pending_tool).
        if (_session is null && !await PickShellDocumentAsync()) return;

        ShowEditorView();
        // Open installs the session before the async permission snapshot reaches
        // the controls. A cold-start Edit must wait for that same owner before arming.
        var sessionId = _session?.SessionId;
        if (_annotationState is null && _session is { PageCount: > 0 }) await RefreshAnnotationStateAsync();
        if (_isBusy || _session?.SessionId != sessionId) return;
        if (destination == "Organize")
        {
            await EnterOrganizeViewAsync();
            return;
        }
        LeaveOrganizeView();
        SetToolsPanelVisible(true);
        if (destination == "Protect")
        {
            if (ProtectButton.IsEnabled) ProtectButton_Click(ProtectButton, e);
            else AnnotationStatus.Text = I18n.T("protect-forbidden");
            return;
        }
        if (destination == "Compress")
        {
            if (CompressButton.IsEnabled) CompressButton_Click(CompressButton, e);
            return;
        }
        SelectToolPage(destination);
        if (destination == "Edit" && ContentEditButton.IsEnabled && ContentEditButton.IsChecked != true)
        {
            ContentEditButton.IsChecked = true;
            ContentEditButton_Click(ContentEditButton, e);
        }
        else if (destination == "Annotate") HighlightButton.Focus(FocusState.Programmatic);
        else if (destination == "Sign") EditFormsButton.Focus(FocusState.Programmatic);
    }
}
