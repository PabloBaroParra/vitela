using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// The mode bar across the top of the window (GTK <c>mode_bar.rs</c>): Home,
/// Edit, Annotate, Organize, Sign, Protect — one row of tabs that replaced
/// the app rail and the tools panel's own tab strip.
///
/// The bar never stores which mode is current. <see cref="SyncModeBar"/>
/// reads it from the three views' visibility and the tools panel's page
/// (<see cref="ShellModes.Current"/>) and runs whenever either moves, so an
/// open landing, Organize's return, the palette or a Home tool changes the
/// checked tab without knowing the bar exists. What a mode does is not defined
/// here either: every document mode is <see cref="NavigateToToolAsync"/>, the
/// same door Home's tool tiles use.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>MODE_ICON_PX: the icon edge on a mode tab, sized against its label.</summary>
    private const double ModeIconPx = 16;

    private readonly List<(ShellMode Mode, ButtonBase Button)> _modeButtons = [];

    private void BuildModeBar()
    {
        ModeBar.ColumnSpacing = 4;
        ModeBar.RowSpacing = 4;
        foreach (var mode in ShellModes.All)
        {
            // Place modes are toggles; Protect opens a dialog over any screen, so it is a plain button.
            ButtonBase button = ShellModes.IsPlace(mode) ? new ToggleButton() : new Button();
            button.Style = NamedStyle("ModeTabStyle");
            var label = ShellModes.Label(mode);
            ToolbarIcon(button, ShellModes.Icon(mode), label, caption: label, size: ModeIconPx);
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(button, $"Mode_{mode}");
            button.Click += async (_, e) => await ModeButton_ClickAsync(mode, e);
            ModeBar.Children.Add(button);
            _modeButtons.Add((mode, button));
        }
        foreach (var view in new UIElement[] { HomeView, EditorView, OrganizePanel })
            view.RegisterPropertyChangedCallback(UIElement.VisibilityProperty, (_, _) => SyncModeBar());
        SyncModeBar();
    }

    /// <summary>
    /// Checks the tab for the screen on display and clears every other one.
    /// Never changes a view: it only reports what the views settled on.
    /// </summary>
    private void SyncModeBar()
    {
        var screen = ShellModes.ScreenOf(
            homeVisible: HomeView.Visibility == Visibility.Visible,
            editorVisible: EditorView.Visibility == Visibility.Visible,
            organizeVisible: OrganizePanel.Visibility == Visibility.Visible);
        var current = ShellModes.Current(screen, _selectedToolPage);
        foreach (var (mode, button) in _modeButtons)
            if (button is ToggleButton toggle) toggle.IsChecked = current == mode;
    }

    private void SetModeBarEnabled(bool enabled)
    {
        foreach (var (_, button) in _modeButtons) button.IsEnabled = enabled;
    }

    /// <summary>The checked tab, for focus to land on when nothing better is on screen.</summary>
    private void FocusCheckedModeButton() =>
        _modeButtons.Select(entry => entry.Button).OfType<ToggleButton>().FirstOrDefault(button => button.IsChecked == true)
            ?.Focus(FocusState.Programmatic);

    private async Task ModeButton_ClickAsync(ShellMode mode, RoutedEventArgs e)
    {
        // The click has already flipped the toggle; say again what the screen says
        // before anything awaits, so a picker or a refusal never shows a stray tab.
        SyncModeBar();
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
        await OpenModeAsync(mode, e);
        SyncModeBar();
    }

    /// <summary>
    /// What a mode's tab does. The palette's mode entries run this too, so a
    /// mode picked there cannot land somewhere its tab would not.
    /// </summary>
    private async Task OpenModeAsync(ShellMode mode, RoutedEventArgs e)
    {
        // Home is navigation only: the open document stays open behind it exactly as it was.
        if (mode == ShellMode.Home) ShowHomeView();
        else await NavigateToToolAsync(mode.ToString(), e);
    }
}
