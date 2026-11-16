using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Tools-page navigation and control placement, never document/edit state.
/// The panel has no tab strip of its own: the mode bar's Edit, Annotate and
/// Sign modes pick the page, and the bar follows <see cref="SelectToolPage"/>
/// back. Comments is a section at the foot of Annotate (GTK tools_panel.rs):
/// the bar has no Comments mode, and a comment is an annotation.
/// </summary>
public sealed partial class MainWindow
{
    private readonly Dictionary<string, StackPanel> _toolPages = [];
    private string _selectedToolPage = ShellModes.AnnotatePage;

    private void BuildToolPages()
    {
        // Edit leads, matching the mode bar and Home's Tools card; Annotate stays the page that opens.
        foreach (var key in new[] { ShellModes.EditPage, ShellModes.AnnotatePage, ShellModes.SignPage })
        {
            _toolPages[key] = new StackPanel { Spacing = 8, Visibility = key == _selectedToolPage ? Visibility.Visible : Visibility.Collapsed };
            Microsoft.UI.Xaml.Automation.AutomationProperties.SetAutomationId(_toolPages[key], $"ToolsPage_{key}");
        }

        var pageIndex = 0;
        foreach (var page in _toolPages.Values) SidebarContent.Children.Insert(pageIndex++, page);
        SidebarContent.Children.Remove(DocumentProperties);
        _toolPages["Annotate"].Children.Add(DocumentProperties);
        SidebarContent.Children.Remove(FormFieldsPanel);
        _toolPages["Sign"].Children.Add(FormFieldsPanel);
        BuildSigningPanel();
        BuildDrawSignaturePanel();
        BuildCommentsPanel();

        // Reuse the same named controls and handlers, including Windows-only operations.
        // Presentation changes must not create a second edit state or command implementation.
        var editControls = new HashSet<UIElement>
        {
            ContentEditButton, ResizeImageButton, MoveImageButton, DeleteImageButton,
            ReplaceImageButton, InsertImageButton, DeleteTextButton, MoveTextButton, InsertTextButton,
            AddWatermarkButton,
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
        DisarmSignatureOutsideSign(key);
        _selectedToolPage = key;
        foreach (var (name, page) in _toolPages)
            page.Visibility = name == key ? Visibility.Visible : Visibility.Collapsed;
        SyncModeBar();
    }
}
