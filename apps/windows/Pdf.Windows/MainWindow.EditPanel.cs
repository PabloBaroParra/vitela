using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;

namespace Pdf.Windows;

/// <summary>Edit-page presentation; existing feature partials retain edit state and commands.</summary>
public sealed partial class MainWindow
{
    private readonly TextBlock _editAvailability = EditHint("Open a PDF to edit the text and images on its pages.");
    private Border? _editNotice;
    private readonly TextBlock _editTextHint = EditHint("Click a text run to retype it in place.");

    private void BuildEditPanel(Viewer.ToolbarPanel row)
    {
        var page = _toolPages["Edit"];
        var heading = new TextBlock { Text = "Edit PDF", Style = NamedStyle("EditPanelHeadingStyle") };
        AutomationProperties.SetAutomationId(heading, "EditPanelHeading");
        page.Children.Add(heading);
        page.Children.Add(EditHint("Change the text and images already on the page."));
        AutomationProperties.SetAutomationId(_editAvailability, "EditAvailability");
        _editAvailability.Style = NamedStyle("EditNoticeTextStyle");
        _editNotice = new Border { Child = _editAvailability, Style = NamedStyle("EditNoticeStyle") };
        page.Children.Add(_editNotice);

        // Explicit collection removal also works before this hidden subtree loads.
        foreach (var control in row.Children.ToArray()) row.Children.Remove(control);
        ToolbarIcon(ContentEditButton, "\uE70F", "Edit content",
            "Turn on content editing, then click a text run on the page", "Edit content");
        ToolbarIcon(InsertTextButton, "\uE8D2", "Insert text", "Insert a new text run using page coordinates", "Insert text");
        ToolbarIcon(DeleteTextButton, "\uE74D", "Delete text", "Remove the text run being edited from the page (not secure redaction)", "Delete text");
        // Keep the inline target when clicked, without removing keyboard access.
        DeleteTextButton.AllowFocusOnInteraction = false;
        ToolbarIcon(InsertImageButton, "\uEB9F", "Insert image", "Choose a picture and its page coordinates", "Insert image");
        ToolbarIcon(ReplaceImageButton, "\uEB9F", "Replace image", "Choose a page image to replace with a file on disk", "Replace image");
        ToolbarIcon(DeleteImageButton, "\uE74D", "Delete image", "Choose a page image to remove", "Delete image");
        // .edit-tile: the cards' buttons are tiles; an armed mode (Edit content) moves its border to the accent.
        ContentEditButton.Style = NamedStyle("EditTileToggleStyle");
        foreach (var tile in new Button[] { InsertTextButton, DeleteTextButton, InsertImageButton, ReplaceImageButton, DeleteImageButton })
            tile.Style = NamedStyle("EditTileButtonStyle");

        page.Children.Add(EditCard("Text", "EditTextCard",
            _editTextHint,
            ContentEditButton, InsertTextButton, DeleteTextButton));
        page.Children.Add(EditCard("Images", "EditImagesCard",
            EditHint("Insert image, Replace image and Delete image currently use dialogs. Choose an image on the visible page; save first if it already has a pending edit. Some encodings cannot be replaced because the swap could not be undone."),
            InsertImageButton, ReplaceImageButton, DeleteImageButton));

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
        if (_editNotice is null) return;
        var reason = _session is null ? "Open a PDF to edit the text and images on its pages."
            : !_session.ContentEditingAllowed ? "This document does not permit content changes."
            : _session.PageCount == 0 ? "This PDF has no pages to edit."
            : _isBusy ? "Please wait while the document is being updated."
            : _organizing ? "Return to the document to edit its text and images." : null;
        _editAvailability.Text = reason ?? string.Empty;
        _editNotice.Visibility = reason is null ? Visibility.Collapsed : Visibility.Visible;
    }

    private void UpdateEditTextSelection()
    {
        var editor = _pump.Box;
        var pending = editor is not null && _pump.WrittenFor(editor.PageIndex, editor.Run.Id) is not null;
        DeleteTextButton.IsEnabled = ContentEditButton.IsEnabled && _contentEditMode
            && editor is not null && !pending && !_deletingContentText;
        _editTextHint.Text = editor is null ? "Click a text run to retype it in place."
            : pending ? "This text has a pending retype — save and reopen before deleting it."
            : "Text run selected — delete it, or retype it in place.";
    }
}
