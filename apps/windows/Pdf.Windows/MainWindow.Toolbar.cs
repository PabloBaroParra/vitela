using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;

namespace Pdf.Windows;

/// <summary>Document-wide command presentation; existing feature handlers own all operations.</summary>
public sealed partial class MainWindow
{
    private readonly Button _findDocumentButton = new();
    private readonly Flyout _documentFindFlyout = new();

    private void BuildEditorToolbar()
    {
        // The initially collapsed sidebar is not loaded: Parent can be null
        // while its XAML collection still owns these buttons.
        SearchMatchControls.Children.Remove(PreviousMatchButton);
        SearchMatchControls.Children.Remove(NextMatchButton);
        SidebarContent.Children.Remove(SearchMatchControls);
        // Unparent the named controls before regrouping: no command or enabled-state owner changes.
        foreach (var control in new FrameworkElement[]
        {
            OpenButton, OpenSampleButton, SaveButton, PrintButton, ExportImagesButton,
            UndoButton, RedoButton, DocumentTitle, PageCounter, FirstPageButton,
            PreviousPageButton, NextPageButton, LastPageButton, GoToPageButton,
            ZoomOutButton, ZoomLevel, ZoomInButton, FitWidthButton, FitPageButton,
            PagesPanelButton, ToolsPanelButton, SearchBox, SearchButton,
            ProtectButton, CompressButton, ExtractPagesButton, SplitPagesButton, OrganizeButton,
            PreviousMatchButton, NextMatchButton,
        })
        {
            if (control.Parent is Panel parent) parent.Children.Remove(control);
        }
        EditorToolbar.Children.Clear();

        ToolbarIcon(OpenButton, "\uE8E5", "Open PDF", "Open PDF (Ctrl+O)", "Open");
        ToolbarIcon(OpenSampleButton, "\uE8A5", "Open sample");
        ToolbarIcon(SaveButton, "\uE74E", "Save as", "Save as (Ctrl+S)");
        ToolbarIcon(PrintButton, "\uE749", "Print", "Print (Ctrl+P)");
        ToolbarIcon(ExportImagesButton, "\uE91B", "Export images", "Export pages as images");
        ToolbarIcon(UndoButton, "\uE7A7", "Undo", "Undo (Ctrl+Z)");
        ToolbarIcon(RedoButton, "\uE7A6", "Redo", "Redo (Ctrl+Y)");
        ToolbarIcon(ZoomOutButton, "\uE71F", "Zoom out");
        ToolbarIcon(ZoomInButton, "\uE8A3", "Zoom in");
        ToolbarIcon(FitWidthButton, "\uE9A9", "Fit width");
        ToolbarIcon(FitPageButton, "\uE740", "Fit page");
        ToolbarIcon(PagesPanelButton, "\uE892", "Pages", "Show or hide the page list");
        ToolbarIcon(ToolsPanelButton, "\uE893", "Tools", "Show or hide tools");
        ToolbarIcon(PreviousMatchButton, "\uE76B", "Previous match");
        ToolbarIcon(NextMatchButton, "\uE76C", "Next match");
        ToolbarIcon(_findDocumentButton, "\uE721", "Find in document", "Find in document (Ctrl+F)");

        ToolTipService.SetToolTip(PageCounter, "Current page / total pages");
        AutomationProperties.SetName(PageCounter, "Current page / total pages");
        AddToolbarGroup("Document", OpenButton, OpenSampleButton);
        AddToolbarGroup("Output", SaveButton, PrintButton, ExportImagesButton);
        AddToolbarGroup("History", UndoButton, RedoButton);
        AddToolbarGroup("Position", PageCounter);
        AddToolbarGroup("Zoom", ZoomOutButton, ZoomLevel, ZoomInButton);
        AddToolbarGroup("Fit", FitWidthButton, FitPageButton);
        AddToolbarGroup("Panels", PagesPanelButton, ToolsPanelButton);
        AddToolbarGroup("Find", _findDocumentButton);

        // Windows already offers these commands. Keep them reachable, separately
        // from GTK's eight primary groups, instead of deleting working behaviour.
        ToolbarIcon(FirstPageButton, "\uE892", "First page", "Go to the first page of the document (Ctrl+Home)");
        ToolbarIcon(PreviousPageButton, "\uE76B", "Previous page", "Go to the page before the current viewport page (Ctrl+Up)");
        ToolbarIcon(NextPageButton, "\uE76C", "Next page", "Go to the page after the current viewport page (Ctrl+Down)");
        ToolbarIcon(LastPageButton, "\uE893", "Last page", "Go to the last page of the document (Ctrl+End)");
        ToolbarIcon(GoToPageButton, "\uE8A9", "Go to page", "Jump to a page by number (Ctrl+G)");
        AddToolbarGroup("Page navigation", FirstPageButton, PreviousPageButton, NextPageButton, LastPageButton, GoToPageButton);
        ToolbarIcon(ProtectButton, "\uE72E", "Protect");
        ToolbarIcon(CompressButton, "\uE8B8", "Compress");
        ToolbarIcon(ExtractPagesButton, "\uE8A5", "Extract pages", "Save chosen pages as a new PDF");
        ToolbarIcon(SplitPagesButton, "\uE8A4", "Split PDF", "Cut the document into several PDFs");
        ToolbarIcon(OrganizeButton, "\uE8A9", "Organize", "Move, rotate or delete pages");
        AddToolbarGroup("More document tools", ProtectButton, CompressButton, ExtractPagesButton, SplitPagesButton, OrganizeButton);
        AddToolbarGroup("Document title", DocumentTitle);

        var searchContent = new StackPanel { Spacing = 8, MaxWidth = 340 };
        searchContent.Children.Add(new TextBlock { Text = "Find in document", FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        SearchBox.PlaceholderText = "Find in document";
        SearchBox.Width = 180;
        AutomationProperties.SetName(SearchBox, "Search document");
        var searchRow = new Viewer.ToolbarPanel();
        searchRow.Children.Add(SearchBox);
        searchRow.Children.Add(PreviousMatchButton);
        searchRow.Children.Add(NextMatchButton);
        searchContent.Children.Add(searchRow);
        searchContent.Children.Add(SearchButton);
        searchContent.Children.Add(new TextBlock { Text = "Case-sensitive. Press Enter to search.", TextWrapping = TextWrapping.Wrap });
        _documentFindFlyout.Content = searchContent;
        _documentFindFlyout.Opened += (_, _) => SearchBox.Focus(FocusState.Programmatic);
        _findDocumentButton.Flyout = _documentFindFlyout;
        AutomationProperties.SetAutomationId(_findDocumentButton, "FindDocumentButton");
        UpdateSearchControls();
    }

    private void AddToolbarGroup(string name, params UIElement[] controls)
    {
        var group = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 2 };
        AutomationProperties.SetName(group, name);
        foreach (var control in controls) group.Children.Add(control);
        EditorToolbar.Children.Add(group);
    }

    private static void ToolbarIcon(ButtonBase button, string glyph, string name, string? tooltip = null, string? caption = null)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        content.Children.Add(new FontIcon { Glyph = glyph, FontSize = 18 });
        if (caption is not null) content.Children.Add(new TextBlock { Text = caption, VerticalAlignment = VerticalAlignment.Center });
        button.Content = content;
        AutomationProperties.SetName(button, name);
        ToolTipService.SetToolTip(button, tooltip ?? name);
    }
}
