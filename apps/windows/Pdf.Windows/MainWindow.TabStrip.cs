using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Localization;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// The tab strip (GTK <c>tabs/strip.rs</c>): one tab per open document above
/// the editor and Organize screens — its name, an unsaved marker and a close
/// button. Hidden on Home and with no document. Redrawn from
/// <see cref="_tabList"/>; it holds no state of its own.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>The marker in front of a tab's name while it has unsaved changes; the accessible name says it in words.</summary>
    private const string UnsavedMarker = "•";

    /// <summary>The close button's face: a symbol rather than an icon lookup.</summary>
    private const string CloseMarker = "×";

    private bool _tabStripSyncQueued;

    /// <summary>What the strip last drew, so a redraw that would change nothing keeps its buttons (and keyboard focus).</summary>
    private string? _tabStripShown;

    /// <summary>
    /// Redraws the strip once the current turn is done. Coalesced: it runs after
    /// every command refresh, and asking each tab whether it is dirty is a
    /// facade round trip.
    /// </summary>
    private void SyncTabStrip()
    {
        UpdateTabStripVisibility();
        if (_tabStripSyncQueued || _windowClosed) return;
        _tabStripSyncQueued = true;
        if (!DispatcherQueue.TryEnqueue(DispatcherQueuePriority.Low, () => _ = RedrawTabStripAsync())) _tabStripSyncQueued = false;
    }

    private async Task RedrawTabStripAsync()
    {
        try
        {
            foreach (var tab in _tabs.Values.ToList()) tab.Dirty = await TabIsDirtyAsync(tab);
        }
        finally
        {
            _tabStripSyncQueued = false;
        }
        if (_windowClosed) return;
        RenderTabStrip();
    }

    private void RenderTabStrip()
    {
        var shown = string.Join("\n", _tabList.Ids.Where(_tabs.ContainsKey).Select(id =>
        {
            var tab = _tabs[id];
            var active = _tabList.Active == id;
            return $"{id.Value}|{active}|{tab.Dirty}|{TabName(tab, active)}|{TabTooltip(tab, active)}";
        }));
        UpdateTabStripVisibility();
        Title = ActiveTab is null || _session is null
            ? "Vitela"
            : I18n.T("tabs-window-title", ("name", _session.DisplayName));
        if (shown == _tabStripShown) return;
        _tabStripShown = shown;
        TabStripPanel.Children.Clear();
        foreach (var id in _tabList.Ids)
        {
            if (!_tabs.TryGetValue(id, out var tab)) continue;
            TabStripPanel.Children.Add(BuildTab(id, tab, _tabList.Active == id));
        }
    }

    private string TabName(DocumentTab tab, bool active) => (active ? _session?.DisplayName : null) ?? tab.Session?.DisplayName ?? "";

    private string TabTooltip(DocumentTab tab, bool active) =>
        (active ? _documentOrigin : tab.Origin) is { Length: > 0 } path ? path : TabName(tab, active);

    private UIElement BuildTab(TabId id, DocumentTab tab, bool active)
    {
        var name = TabName(tab, active);
        var label = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 4 };
        if (tab.Dirty)
        {
            label.Children.Add(new TextBlock { Text = UnsavedMarker, Foreground = ThemeBrush("VitelaAccentBrush"), VerticalAlignment = VerticalAlignment.Center });
        }
        label.Children.Add(new TextBlock
        {
            Text = name,
            MaxWidth = 220,
            TextTrimming = TextTrimming.CharacterEllipsis,
            VerticalAlignment = VerticalAlignment.Center,
            FontWeight = active ? Microsoft.UI.Text.FontWeights.SemiBold : Microsoft.UI.Text.FontWeights.Normal,
        });
        var select = new Button
        {
            Content = label,
            Padding = new Thickness(8, 4, 4, 4),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
        };
        AutomationProperties.SetName(select, tab.Dirty ? I18n.T("tabs-tab-unsaved", ("name", name)) : name);
        ToolTipService.SetToolTip(select, TabTooltip(tab, active));
        select.Click += async (_, _) => await RequestTabActivationAsync(id);

        var close = new Button
        {
            Content = CloseMarker,
            Padding = new Thickness(6, 2, 6, 2),
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderThickness = new Thickness(0),
            VerticalAlignment = VerticalAlignment.Center,
        };
        AutomationProperties.SetName(close, I18n.T("tabs-close-named", ("name", name)));
        ToolTipService.SetToolTip(close, I18n.T("tabs-close-tooltip"));
        close.Click += async (_, _) => await RequestCloseTabAsync(id);

        var row = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 0 };
        row.Children.Add(select);
        row.Children.Add(close);
        return new Border
        {
            Child = row,
            CornerRadius = new CornerRadius(6, 6, 0, 0),
            BorderThickness = new Thickness(1, 1, 1, 0),
            Background = active ? ThemeBrush("VitelaWindowBackgroundBrush") : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            BorderBrush = active ? ThemeBrush("VitelaStrongSeparatorBrush") : new SolidColorBrush(Microsoft.UI.Colors.Transparent),
        };
    }

    /// <summary>The strip shows while there are tabs and the screen is not Home.</summary>
    private void UpdateTabStripVisibility()
    {
        if (TabStrip is null) return;
        TabStrip.Visibility = !_tabList.IsEmpty && HomeView.Visibility != Visibility.Visible
            ? Visibility.Visible
            : Visibility.Collapsed;
    }
}
