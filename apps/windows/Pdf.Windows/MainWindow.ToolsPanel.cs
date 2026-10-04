using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Tools-page navigation and control placement, never document/edit state.</summary>
public sealed partial class MainWindow
{
    private readonly Dictionary<string, StackPanel> _toolPages = [];
    private readonly Viewer.ToolbarPanel _toolsTabStrip = new();

    private void BuildToolPages()
    {
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetName(_toolsTabStrip, "Tools pages");
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(_toolsTabStrip, "ToolsTabStrip");
        // Edit leads, matching the rail and Home's Tools card; Annotate stays the page that opens.
        foreach (var (key, label, icon) in new[]
        {
            ("Edit", "Edit", ShellIcon.Edit), ("Annotate", "Annotate", ShellIcon.Annotate),
            ("Comments", "Comments", ShellIcon.Comments), ("Sign", "Fill & Sign", ShellIcon.Sign),
        })
        {
            var tab = new ToggleButton { Content = label, Tag = key, IsChecked = key == "Annotate" };
            ToolbarIcon(tab, icon, label, caption: label, size: 16);
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(tab, $"ToolsTab_{key}");
            tab.Click += (_, _) => SelectToolPage(key);
            _toolsTabStrip.Children.Add(tab);
            _toolPages[key] = new StackPanel { Spacing = 8, Visibility = key == "Annotate" ? Visibility.Visible : Visibility.Collapsed };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(_toolPages[key], $"ToolsPage_{key}");
        }

        SidebarContent.Children.Insert(0, _toolsTabStrip);
        var pageIndex = 1;
        foreach (var page in _toolPages.Values) SidebarContent.Children.Insert(pageIndex++, page);
        SidebarContent.Children.Remove(DocumentProperties);
        _toolPages["Annotate"].Children.Add(DocumentProperties);
        SidebarContent.Children.Remove(FormFieldsPanel);
        _toolPages["Sign"].Children.Add(FormFieldsPanel);
        BuildSigningPanel();
        var comments = new TextBlock
        {
            Text = "Comments aren't available in this shell yet.", TextWrapping = TextWrapping.Wrap,
            VerticalAlignment = VerticalAlignment.Top,
            Foreground = (Microsoft.UI.Xaml.Media.Brush)Application.Current.Resources["TextFillColorSecondaryBrush"],
        };
        Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(comments, "CommentsPlaceholder");
        _toolPages["Comments"].Children.Add(comments);

        // Reuse the same named controls and handlers, including Windows-only operations.
        // Presentation changes must not create a second edit state or command implementation.
        var editControls = new HashSet<UIElement>
        {
            ContentEditButton, ResizeImageButton, MoveImageButton, DeleteImageButton,
            ReplaceImageButton, InsertImageButton, DeleteTextButton, MoveTextButton, InsertTextButton,
        };
        var annotateRow = new Viewer.ToolbarPanel();
        var editRow = new Viewer.ToolbarPanel();
        foreach (var control in EditingToolbar.Children.ToArray())
        {
            EditingToolbar.Children.Remove(control);
            (editControls.Contains(control) ? editRow : annotateRow).Children.Add(control);
        }
        EditingToolbar.Visibility = Visibility.Collapsed;
        BuildAnnotationPanel(annotateRow);
        BuildEditPanel(editRow);
    }

    private void SelectToolPage(string key)
    {
        if (!_toolPages.ContainsKey(key)) return;
        foreach (var (name, page) in _toolPages)
            page.Visibility = name == key ? Visibility.Visible : Visibility.Collapsed;
        foreach (var tab in _toolsTabStrip.Children.OfType<ToggleButton>())
            tab.IsChecked = (string)tab.Tag == key;
    }
}
