using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Home presentation and filtering; tool dispatch remains shared with the rail.</summary>
public sealed partial class MainWindow
{
    private readonly List<Button> _homeToolTiles = [];
    private Border? _homeToolsCard;

    private void BuildHomeTools()
    {
        var grid = new Grid { RowSpacing = 8, ColumnSpacing = 8 };
        for (var column = 0; column < 3; column++) grid.ColumnDefinitions.Add(new ColumnDefinition());
        for (var row = 0; row < 2; row++) grid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        var entries = new[]
        {
            ("Edit", "Retype text and replace images", ShellIcon.Edit, IconTint.Edit),
            ("Annotate", "Highlight, draw, and add notes", ShellIcon.Annotate, IconTint.Annotate),
            ("Sign", "Sign with a certificate, card, or token", ShellIcon.Sign, IconTint.Sign),
            ("Organize", "Reorder and delete pages", ShellIcon.Organize, IconTint.Organize),
            ("Compress", "Write a smaller copy of the file", ShellIcon.Compress, IconTint.Compress),
            ("Protect", "Require a password to open the document", ShellIcon.Protect, IconTint.Protect),
        };
        for (var index = 0; index < entries.Length; index++)
        {
            var (label, description, icon, tint) = entries[index];
            var content = new StackPanel { Spacing = 6, HorizontalAlignment = HorizontalAlignment.Center };
            var tile = new Button { Tag = label, Style = HomeStyle("ToolTileButtonStyle") };
            // TILE_ICON_PX in the tool's own hue; a disabled tile goes muted (home/tools.rs).
            content.Children.Add(ShellIconImage(icon, 24, tint, tile));
            content.Children.Add(new TextBlock { Text = label, TextWrapping = TextWrapping.Wrap, TextAlignment = TextAlignment.Center });
            tile.Content = content;
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(tile, label);
            ToolTipService.SetToolTip(tile, description);
            tile.Click += async (_, e) => await NavigateToToolAsync(label, e);
            Grid.SetColumn(tile, index % 3);
            Grid.SetRow(tile, index / 3);
            grid.Children.Add(tile);
            _homeToolTiles.Add(tile);
        }
        HomeVersionText.Text = AppVersion.FooterText;
        _homeToolsCard = HomeCard("Tools", grid);
        HomeToolsColumn.Children.Add(_homeToolsCard);
        BuildHomeQuickActions();
        ApplyHomeFilter();
    }

    private void BuildHomeQuickActions()
    {
        var actions = new StackPanel { Spacing = 8 };
        foreach (var (label, icon, run) in new (string, ShellIcon, Func<Task>)[]
        {
            ("New blank PDF", ShellIcon.NewFile, CreateNewDocumentAsync),
            ("Open file…", ShellIcon.Files, async () => { await PickShellDocumentAsync(); }),
            ("Open the sample", ShellIcon.Sample, async () =>
            {
                if (!_isBusy && !_dialogOpen && !_shellPickingFile) await OpenSampleFileAsync(SamplePath, SampleDisplayName);
            }),
        })
        {
            var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
            var button = new Button { Style = HomeStyle("HomeLinkButtonStyle") };
            // ROW_ICON_PX in ACCENT_TINT (home/tools.rs).
            row.Children.Add(ShellIconImage(icon, 16, IconTint.Accent, button));
            row.Children.Add(new TextBlock { Text = label });
            button.Content = row;
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(button, label);
            button.Click += async (_, _) => await run();
            actions.Children.Add(button);
        }
        HomeToolsColumn.Children.Add(HomeCard("Quick actions", actions));

        var shortcuts = new StackPanel { Spacing = 6 };
        foreach (var (action, keys) in new[] { ("Open", "Ctrl+O"), ("New", "Ctrl+N"), ("Save", "Ctrl+S"), ("Find", "Ctrl+F"), ("Print", "Ctrl+P") })
        {
            var row = new Grid();
            row.ColumnDefinitions.Add(new ColumnDefinition());
            row.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            // .property-key / .property-value
            row.Children.Add(new TextBlock { Text = action, FontSize = 12, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold, Style = HomeStyle("HomeSubtitleStyle") });
            var keyLabel = new TextBlock { Text = keys, FontSize = 12, Foreground = ThemeBrush("VitelaTextPrimaryBrush") };
            Grid.SetColumn(keyLabel, 1);
            row.Children.Add(keyLabel);
            shortcuts.Children.Add(row);
        }
        HomeToolsColumn.Children.Add(HomeCard("Keyboard shortcuts", shortcuts));
    }

    private static Border HomeCard(string title, UIElement content)
    {
        var panel = new StackPanel { Spacing = 10 };
        panel.Children.Add(new TextBlock { Text = title, Style = HomeStyle("HomeCardTitleStyle") });
        panel.Children.Add(content);
        return new Border { Child = panel, Style = HomeStyle("HomeCardStyle") };
    }

    /// <summary>A named style from Themes/HomeStyles.xaml; the class-for-class port of HOME_CSS.</summary>
    internal static Style HomeStyle(string key) => NamedStyle(key);

    /// <summary>Any named style from Themes/*.xaml.</summary>
    internal static Style NamedStyle(string key) => (Style)Application.Current.Resources[key];

    /// <summary>
    /// A palette brush for the current system theme. Only for values set once
    /// while building code-made UI; anything in XAML uses {ThemeResource}.
    /// </summary>
    internal static Brush ThemeBrush(string key) => (Brush)Application.Current.Resources[key];

    private bool? _homeStacked;

    /// <summary>
    /// Linux lets Home's body scroll sideways below its minimum; a WinUI
    /// ScrollViewer cannot do that without measuring its children at infinite
    /// width, so the tools column drops under the main one instead.
    /// </summary>
    private void HomeBody_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        var stacked = Viewer.HomeLayout.IsStacked(e.NewSize.Width);
        if (_homeStacked == stacked) return;
        _homeStacked = stacked;
        HomeBody.ColumnDefinitions[1].Width = stacked ? new GridLength(0) : new GridLength(Viewer.HomeLayout.SideColumnWidth);
        HomeBody.ColumnSpacing = stacked ? 0 : 20;
        HomeBody.RowSpacing = stacked ? 20 : 0;
        Grid.SetColumn(HomeToolsColumn, stacked ? 0 : 1);
        Grid.SetRow(HomeToolsColumn, stacked ? 1 : 0);
    }

    private void HomeSearchBox_TextChanged(object sender, TextChangedEventArgs e) => ApplyHomeFilter();

    private async void HomeDropZone_Tapped(object sender, Microsoft.UI.Xaml.Input.TappedRoutedEventArgs e)
    {
        // The nested real button owns its click. Never raise two pickers for one tap.
        for (var element = e.OriginalSource as DependencyObject; element is not null; element = VisualTreeHelper.GetParent(element))
            if (ReferenceEquals(element, HomeSelectFileButton)) return;
        e.Handled = true;
        await PickShellDocumentAsync();
    }

    private void HomeDropZone_DragOver(object sender, DragEventArgs e)
    {
        if (_isBusy || _shellPickingFile || _dialogOpen) return;
        AcceptFileDrop(e);
        SetDropZoneActive(true);
    }

    private void HomeDropZone_DragLeave(object sender, DragEventArgs e) => SetDropZoneActive(false);

    private void HomeDropZone_Drop(object sender, DragEventArgs e)
    {
        SetDropZoneActive(false);
        Window_Drop(sender, e);
    }

    /// <summary>.home-dropzone.drop-active: soft accent fill and a solid accent outline while a drag is over.</summary>
    private void SetDropZoneActive(bool active)
    {
        HomeDropZone.Background = ThemeBrush(active ? "VitelaAccentSoftBrush" : "VitelaEditorBackgroundBrush");
        HomeDropZoneOutline.Stroke = ThemeBrush(active ? "VitelaAccentBrush" : "VitelaStrongSeparatorBrush");
    }

    private void ApplyHomeFilter()
    {
        if (HomeSearchBox is null) return;
        var query = HomeSearchBox.Text;
        foreach (var tile in _homeToolTiles)
            tile.Visibility = ((string)tile.Tag).Contains(query, StringComparison.OrdinalIgnoreCase) ? Visibility.Visible : Visibility.Collapsed;
        if (_homeToolsCard is not null)
            _homeToolsCard.Visibility = _homeToolTiles.Any(tile => tile.Visibility == Visibility.Visible) ? Visibility.Visible : Visibility.Collapsed;
        if (RecentFilesList is null) return;
        FilterRecentFiles(query);
    }
}
