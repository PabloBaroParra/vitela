using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>Window-local view navigation. Switching Home never replaces a session.</summary>
public sealed partial class MainWindow
{
    private bool _shellInitialized;
    private bool _shellPickingFile;
    private string _railDestination = "Home";

    private void ShellRoot_Loaded(object sender, RoutedEventArgs e)
    {
        if (_shellInitialized) return;
        _shellInitialized = true;
        foreach (var button in AppRail.Children.OfType<ToggleButton>())
        {
            var label = (string)button.Content;
            var glyph = (string)button.Tag switch
            {
                "Home" => "\uE80F", "Recent" => "\uE823", "Files" => "\uE8B7",
                "Annotate" => "\uE7E6", "Edit" => "\uE70F", "Organize" => "\uE8A9",
                "Sign" => "\uE77F", _ => "\uE72E",
            };
            var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            row.Children.Add(new FontIcon { Glyph = glyph, FontSize = 16 });
            row.Children.Add(new TextBlock { Text = label, VerticalAlignment = VerticalAlignment.Center });
            button.Content = row;
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, label);
        }
        BuildEditorToolbar();
        BuildToolPages();
        BuildHomeTools();
        InitializeRecents();
        SetBusy(false);
        // Mirror the existing operation guard rather than introduce another busy owner.
        OpenButton.RegisterPropertyChangedCallback(Control.IsEnabledProperty, (_, _) =>
            SetNavigationEnabled(OpenButton.IsEnabled && !_shellPickingFile));
    }

    private void SetNavigationEnabled(bool enabled)
    {
        foreach (var button in AppRail.Children.OfType<ToggleButton>()) button.IsEnabled = enabled;
        HomeView.IsEnabled = enabled;
    }

    private void MarkRailDestination(string destination)
    {
        _railDestination = destination;
        foreach (var button in AppRail.Children.OfType<ToggleButton>())
            button.IsChecked = (string)button.Tag == destination;
    }

    private void ShowHomeView(string destination)
    {
        EditorView.Visibility = Visibility.Collapsed;
        OrganizePanel.Visibility = Visibility.Collapsed;
        HomeView.Visibility = Visibility.Visible;
        ReturnToDocumentButton.Visibility = _session is null ? Visibility.Collapsed : Visibility.Visible;
        MarkRailDestination(destination);
        _ = RefreshRecentFilesAsync();
        if (destination == "Recent") FocusFirstRecentCard();
    }

    private void ShowEditorView()
    {
        _recentGeneration++;
        HomeView.Visibility = Visibility.Collapsed;
        EditorView.Visibility = _organizing ? Visibility.Collapsed : Visibility.Visible;
        OrganizePanel.Visibility = _organizing ? Visibility.Visible : Visibility.Collapsed;
    }

    private void ReturnToDocument_Click(object sender, RoutedEventArgs e)
    {
        ShowEditorView();
        MarkRailDestination(_organizing ? "Organize" : "Annotate");
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
            var file = await picker.PickSingleFileAsync();
            if (file is null) return false;
            var previous = _session;
            await OpenStorageFileAsync(file);
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

    private async void RailButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        var destination = (string)((ToggleButton)sender).Tag;
        if (destination is "Home" or "Recent")
        {
            ShowHomeView(destination);
            return;
        }
        if (destination == "Files")
        {
            var previous = _railDestination;
            MarkRailDestination(await PickShellDocumentAsync() ? "Files" : previous);
            return;
        }
        await NavigateToToolAsync(destination, e);
    }

    private async Task NavigateToToolAsync(string destination, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        if (_session is null && !await PickShellDocumentAsync())
        {
            MarkRailDestination(_railDestination);
            return;
        }

        ShowEditorView();
        MarkRailDestination(destination);
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
            else AnnotationStatus.Text = "This document does not permit changes to its protection.";
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
