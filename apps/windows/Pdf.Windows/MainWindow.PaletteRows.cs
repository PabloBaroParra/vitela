using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>The command palette's rows (port of the GTK <c>palette/view.rs</c> row): presentation only.</summary>
public sealed partial class MainWindow
{
    /// <summary>
    /// One command's row: its title, its shortcut from <see cref="ShellShortcuts"/>
    /// when it has one, and — refused — the reason under a dimmed title. A
    /// refused row stays selectable so the reason can be read.
    /// </summary>
    private static ListViewItem PaletteRow(PaletteResult result)
    {
        var entry = result.Entry;
        var refused = result.Refusal is not null;
        var line = new Grid { ColumnSpacing = 12 };
        line.ColumnDefinitions.Add(new ColumnDefinition());
        line.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        line.Children.Add(new TextBlock
        {
            Text = entry.Title,
            TextTrimming = TextTrimming.CharacterEllipsis,
            Foreground = ThemeBrush(refused ? "VitelaMutedBrush" : "VitelaTextPrimaryBrush"),
        });
        if (entry.Shortcut is { } shortcut)
        {
            var keys = new TextBlock { Text = shortcut.Label, FontSize = 12, VerticalAlignment = VerticalAlignment.Center, Foreground = ThemeBrush("VitelaTextSecondaryBrush") };
            Grid.SetColumn(keys, 1);
            line.Children.Add(keys);
        }

        var column = new StackPanel { Spacing = 2 };
        column.Children.Add(line);
        if (result.Refusal is { } reason)
            column.Children.Add(new TextBlock { Text = reason, FontSize = 12, TextWrapping = TextWrapping.Wrap, Foreground = ThemeBrush("VitelaTextSecondaryBrush") });

        var row = new ListViewItem { Content = column, HorizontalContentAlignment = HorizontalAlignment.Stretch, Tag = entry.Id };
        AutomationProperties.SetAutomationId(row, $"PaletteEntry_{entry.Id}");
        AutomationProperties.SetName(row, result.Refusal is { } why
            ? $"{entry.Title}, unavailable: {why}"
            : entry.Shortcut is { } keysFor ? $"{entry.Title}, {keysFor.Label}" : entry.Title);
        return row;
    }
}
