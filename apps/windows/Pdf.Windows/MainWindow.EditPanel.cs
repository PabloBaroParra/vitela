using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Localization;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Edit-page presentation; existing feature partials retain edit state and commands.</summary>
public sealed partial class MainWindow
{
    /// <summary>EDIT_ICON_PX (content_edit/panel.rs): the cards' tiles carry 16px accent icons, muted when unavailable.</summary>
    private const double EditIconSize = 16;

    private readonly TextBlock _editAvailability = EditHint(I18n.T("edit-no-document"));
    private Border? _editNotice;
    private readonly TextBlock _editTextHint = EditHint(I18n.T("edit-text-run-hint"));
    private readonly TextBlock _editImageHint = EditHint(NoImageSelectedHint);

    private void BuildEditPanel(Viewer.ToolbarPanel row)
    {
        var page = _toolPages["Edit"];
        var heading = new TextBlock { Text = I18n.T("edit-heading"), Style = NamedStyle("EditPanelHeadingStyle") };
        AutomationProperties.SetAutomationId(heading, "EditPanelHeading");
        page.Children.Add(heading);
        page.Children.Add(EditHint(I18n.T("edit-description-text-images")));
        AutomationProperties.SetAutomationId(_editAvailability, "EditAvailability");
        _editAvailability.Style = NamedStyle("EditNoticeTextStyle");
        _editNotice = new Border { Child = _editAvailability, Style = NamedStyle("EditNoticeStyle") };
        page.Children.Add(_editNotice);

        // Explicit collection removal also works before this hidden subtree loads.
        foreach (var control in row.Children.ToArray()) row.Children.Remove(control);
        ToolbarIcon(ContentEditButton, ShellIcon.Edit, I18n.T("edit-mode"),
            I18n.T("edit-mode-run-tooltip"), I18n.T("edit-mode"), EditIconSize, IconTint.Accent);
        ToolbarIcon(InsertTextButton, ShellIcon.Text, I18n.T("edit-insert-text"), I18n.T("edit-insert-text-tooltip"),
            I18n.T("edit-insert-text"), EditIconSize, IconTint.Accent);
        ToolbarIcon(DeleteTextButton, ShellIcon.Delete, I18n.T("edit-delete-text"), I18n.T("edit-delete-text-run-tooltip"),
            I18n.T("edit-delete-text"), EditIconSize, IconTint.Accent);
        // Keep the inline target when clicked, without removing keyboard access.
        DeleteTextButton.AllowFocusOnInteraction = false;
        WireTextBlockKeys();
        BuildQuickActions();
        ToolbarIcon(InsertImageButton, ShellIcon.Image, I18n.T("edit-insert-image"), I18n.T("edit-insert-image-tooltip"),
            I18n.T("edit-insert-image"), EditIconSize, IconTint.Accent);
        ToolbarIcon(ReplaceImageButton, ShellIcon.Image, I18n.T("edit-replace-image"), I18n.T("edit-replace-image-tooltip"),
            I18n.T("edit-replace-image"), EditIconSize, IconTint.Accent);
        ToolbarIcon(DeleteImageButton, ShellIcon.Delete, I18n.T("edit-delete-image"), I18n.T("edit-delete-image-tooltip"),
            I18n.T("edit-delete-image"), EditIconSize, IconTint.Accent);
        // .edit-tile: the cards' buttons are tiles; an armed mode (Edit content, Insert text/image) moves its border to the accent.
        foreach (var mode in new[] { ContentEditButton, InsertTextButton, InsertImageButton })
            mode.Style = NamedStyle("EditTileToggleStyle");
        ToolbarIcon(AddWatermarkButton, ShellIcon.Text, I18n.T("edit-watermark"), I18n.T("edit-watermark-picture-tooltip"),
            I18n.T("edit-watermark"), EditIconSize, IconTint.Accent);
        foreach (var tile in new Button[] { DeleteTextButton, ReplaceImageButton, DeleteImageButton, AddWatermarkButton })
            tile.Style = NamedStyle("EditTileButtonStyle");

        page.Children.Add(EditCard(I18n.T("edit-card-text"), "EditTextCard",
            _editTextHint,
            ContentEditButton, InsertTextButton, DeleteTextButton, BuildTextStyleToggles()));
        page.Children.Add(EditCard(I18n.T("edit-card-images"), "EditImagesCard",
            _editImageHint,
            InsertImageButton, ReplaceImageButton, DeleteImageButton));
        page.Children.Add(BuildShapesCard());
        page.Children.Add(EditCard(I18n.T("edit-card-pages"), "EditPagesCard",
            EditHint(I18n.T("edit-watermark-hint")),
            AddWatermarkButton));

        // Preserve working Windows-only numeric geometry commands below the primary cards.
        var more = new Viewer.ToolbarPanel();
        more.Children.Add(MoveTextButton);
        more.Children.Add(MoveImageButton);
        more.Children.Add(ResizeImageButton);
        page.Children.Add(more);
        UpdateEditPanelAvailability();
    }

    private static Border EditCard(string title, string id, TextBlock hint, params UIElement[] controls)
    {
        var content = new StackPanel { Spacing = 8 };
        var heading = new TextBlock { Text = title, Style = NamedStyle("EditCardTitleStyle") };
        AutomationProperties.SetAutomationId(heading, $"{id}Heading");
        content.Children.Add(heading);
        var row = new Viewer.ToolbarPanel();
        foreach (var control in controls) row.Children.Add(control);
        content.Children.Add(row);
        AutomationProperties.SetAutomationId(hint, $"{id}Hint");
        content.Children.Add(hint);
        var card = EditCardBorder(content);
        AutomationProperties.SetAutomationId(card, id);
        return card;
    }

    private static TextBlock EditHint(string text) => new() { Text = text, Style = NamedStyle("EditHintStyle") };

    private static Border EditCardBorder(UIElement child) => new() { Child = child, Style = NamedStyle("EditCardStyle") };

    private void UpdateEditPanelAvailability()
    {
        UpdateEditTextSelection();
        UpdateImageCard();
        UpdateShapeCard();
        if (_editNotice is null) return;
        var reason = _session is null ? I18n.T("edit-no-document")
            : !_session.ContentEditingAllowed ? I18n.T("refusal-content-forbidden")
            : _session.PageCount == 0 ? I18n.T("edit-no-pages")
            : _isBusy ? I18n.T("edit-busy")
            : _organizing ? I18n.T("edit-return-to-document") : null;
        _editAvailability.Text = reason ?? string.Empty;
        _editNotice.Visibility = reason is null ? Visibility.Collapsed : Visibility.Visible;
    }

    private void UpdateEditTextSelection()
    {
        var editor = _pump.Box;
        var block = editor is null ? _selectedTextBlock : null;
        // Inserted text can be deleted before a save: the core writes the
        // insertion into a new in-memory base and deletes it there, or says
        // why it cannot.
        DeleteTextButton.IsEnabled = ContentEditButton.IsEnabled && _contentEditMode && !_deletingContentText && !_isBusy
            && (editor is not null || block is not null);
        _editTextHint.Text = editor is not null
            ? I18n.T("edit-text-run-selected")
            : block is not null ? TextBlockSelectedHint
            : I18n.T("edit-no-text-selected-retype");
        UpdateTextStyleToggles();
        ScheduleQuickActions();
    }
}
