using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Pdf.Windows.Localization;
using Pdf.Windows.Viewer;

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
            OpenButton, OpenSampleButton, SaveButton, PrintButton, ShareButton, ExportImagesButton,
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

        ToolbarIcon(OpenButton, ShellIcon.Files, I18n.T("editor-open"), I18n.T("editor-open-tooltip"), I18n.T("editor-open-short"));
        ToolbarIcon(OpenSampleButton, ShellIcon.Sample, I18n.T("editor-sample"));
        ToolbarIcon(SaveButton, ShellIcon.Save, I18n.T("editor-save"), I18n.T("editor-save-tooltip"));
        ToolbarIcon(PrintButton, ShellIcon.Print, I18n.T("editor-print"), I18n.T("editor-print-tooltip"));
        ToolbarIcon(ShareButton, ShellIcon.Share, I18n.T("editor-share"), I18n.T("editor-share-tooltip"));
        ToolbarIcon(ExportImagesButton, ShellIcon.ExportImages, I18n.T("editor-export"), I18n.T("editor-export-tooltip"));
        ToolbarIcon(UndoButton, ShellIcon.Undo, I18n.T("editor-undo"), I18n.T("editor-undo-tooltip"));
        ToolbarIcon(RedoButton, ShellIcon.Redo, I18n.T("editor-redo"), I18n.T("editor-redo-tooltip"));
        ToolbarIcon(ZoomOutButton, ShellIcon.ZoomOut, I18n.T("editor-zoom-out"));
        ToolbarIcon(ZoomInButton, ShellIcon.ZoomIn, I18n.T("editor-zoom-in"));
        ToolbarIcon(FitWidthButton, ShellIcon.FitWidth, I18n.T("editor-fit-width"));
        ToolbarIcon(FitPageButton, ShellIcon.FitPage, I18n.T("editor-fit-page"));
        ToolbarIcon(PagesPanelButton, ShellIcon.PanelLeft, I18n.T("editor-pages"), I18n.T("editor-pages-tooltip"));
        ToolbarIcon(ToolsPanelButton, ShellIcon.PanelRight, I18n.T("editor-tools"), I18n.T("editor-tools-tooltip"));
        ToolbarIcon(PreviousMatchButton, ShellIcon.Previous, I18n.T("editor-find-previous"));
        ToolbarIcon(NextMatchButton, ShellIcon.Next, I18n.T("editor-find-next"));
        ToolbarIcon(_findDocumentButton, ShellIcon.Search, I18n.T("editor-find"), I18n.T("editor-find-tooltip"));

        ToolTipService.SetToolTip(PageCounter, I18n.T("editor-page-position-tooltip"));
        AutomationProperties.SetName(PageCounter, I18n.T("editor-page-position-tooltip"));
        AddToolbarGroup(I18n.T("editor-group-document"), OpenButton, OpenSampleButton);
        AddToolbarGroup(I18n.T("editor-group-output"), SaveButton, PrintButton, ShareButton, ExportImagesButton);
        AddToolbarGroup(I18n.T("editor-group-history"), UndoButton, RedoButton);
        AddToolbarGroup(I18n.T("editor-group-position"), PageCounter);
        AddToolbarGroup(I18n.T("editor-group-zoom"), ZoomOutButton, ZoomLevel, ZoomInButton);
        AddToolbarGroup(I18n.T("editor-group-fit"), FitWidthButton, FitPageButton);
        AddToolbarGroup(I18n.T("editor-group-panels"), PagesPanelButton, ToolsPanelButton);
        AddToolbarGroup(I18n.T("editor-group-find"), _findDocumentButton);

        // Windows already offers these commands. Keep them reachable, separately
        // from GTK's eight primary groups, instead of deleting working behaviour.
        ToolbarIcon(FirstPageButton, ShellIcon.FirstPage, I18n.T("editor-first-page"), I18n.T("editor-first-page-tooltip"));
        ToolbarIcon(PreviousPageButton, ShellIcon.Previous, I18n.T("editor-previous-page"), I18n.T("editor-previous-page-tooltip"));
        ToolbarIcon(NextPageButton, ShellIcon.Next, I18n.T("editor-next-page"), I18n.T("editor-next-page-tooltip"));
        ToolbarIcon(LastPageButton, ShellIcon.LastPage, I18n.T("editor-last-page"), I18n.T("editor-last-page-tooltip"));
        ToolbarIcon(GoToPageButton, ShellIcon.GoToPage, I18n.T("editor-go-to-page"), I18n.T("editor-go-to-page-tooltip"));
        AddToolbarGroup(I18n.T("editor-group-page-navigation"), FirstPageButton, PreviousPageButton, NextPageButton, LastPageButton, GoToPageButton);
        ToolbarIcon(ProtectButton, ShellIcon.Protect, I18n.T("editor-protect"));
        ToolbarIcon(CompressButton, ShellIcon.Compress, I18n.T("editor-compress"));
        ToolbarIcon(ExtractPagesButton, ShellIcon.ExtractPages, I18n.T("editor-extract-pages"), I18n.T("organize-extract-tooltip"));
        ToolbarIcon(SplitPagesButton, ShellIcon.SplitPages, I18n.T("editor-split-pages"), I18n.T("editor-split-pages-tooltip"));
        ToolbarIcon(OrganizeButton, ShellIcon.Organize, I18n.T("editor-organize"), I18n.T("editor-organize-tooltip"));
        AddToolbarGroup(I18n.T("editor-group-more-tools"), ProtectButton, CompressButton, ExtractPagesButton, SplitPagesButton, OrganizeButton);
        AddToolbarGroup(I18n.T("editor-group-document-title"), DocumentTitle);

        var searchContent = new StackPanel { Spacing = 8, MaxWidth = 340 };
        searchContent.Children.Add(new TextBlock { Text = I18n.T("editor-find"), FontWeight = Microsoft.UI.Text.FontWeights.SemiBold });
        SearchBox.PlaceholderText = I18n.T("editor-find");
        SearchBox.Width = 180;
        AutomationProperties.SetName(SearchBox, I18n.T("editor-search-document"));
        var searchRow = new Viewer.ToolbarPanel();
        searchRow.Children.Add(SearchBox);
        searchRow.Children.Add(PreviousMatchButton);
        searchRow.Children.Add(NextMatchButton);
        searchContent.Children.Add(searchRow);
        searchContent.Children.Add(SearchButton);
        searchContent.Children.Add(new TextBlock { Text = I18n.T("editor-find-hint"), TextWrapping = TextWrapping.Wrap });
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

    /// <summary>Icon plus optional caption: 18px neutral as editor_toolbar.rs draws it, or the caller's size and tint.</summary>
    private static void ToolbarIcon(ButtonBase button, ShellIcon icon, string name, string? tooltip = null, string? caption = null,
        double size = 18, IconTint tint = IconTint.Neutral)
    {
        var content = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 6 };
        content.Children.Add(ShellIconImage(icon, size, tint, button));
        if (caption is not null) content.Children.Add(new TextBlock { Text = caption, VerticalAlignment = VerticalAlignment.Center });
        button.Content = content;
        AutomationProperties.SetName(button, name);
        ToolTipService.SetToolTip(button, tooltip ?? name);
    }
}
