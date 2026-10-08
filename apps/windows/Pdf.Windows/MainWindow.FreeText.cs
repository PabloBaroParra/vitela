using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Text-box (FreeText) annotations: collects the text in a dialog, then lets
/// the core record the box. The dialog stays open on a refusal — a character
/// the font cannot show is named and the typed text is kept — and a result for
/// a document that is no longer on screen is dropped without a trace.
/// </summary>
public sealed partial class MainWindow
{
    private void FreeTextButton_Click(object sender, RoutedEventArgs e) => Arm(FreeTextButton.IsChecked == true ? AnnotationKind.FreeText : null);

    private async void EditFreeTextButton_Click(object sender, RoutedEventArgs e) => await EditSelectedFreeTextAsync();

    /// <summary>
    /// Places a box where the gesture ended: a click hangs the default box from
    /// the click point, a drag draws its own. The shell picks the rect (the core
    /// does not own a default size); the core wraps and validates the text.
    /// </summary>
    private async Task PlaceFreeTextAsync(uint pageIndex, AnnotationPoint origin, AnnotationPoint release)
    {
        if (_session is not { } session || _isBusy || _organizing || _annotationState?.EditingAllowed != true) return;
        var (width, height) = FreeTextInput.UnrotatedPageSize(session.Pages[(int)pageIndex]);
        var rect = FreeTextInput.PlacementRect(origin, release, width, height);
        var color = new PdfCoreColor(0, 0, 0);

        await PromptFreeTextAsync($"Add text box — page {pageIndex + 1}", "Add", string.Empty, "Text box placement canceled.", session,
            text => CommitFreeTextAsync(session, new PdfCoreEdit.Add(PdfCoreAnnotationKind.FreeText, pageIndex,
                new PdfCoreRect(rect.X, rect.Y, rect.Width, rect.Height), color, Contents: text), "Text box added. Changes are pending save.", selectNew: true));
    }

    private async Task EditSelectedFreeTextAsync()
    {
        if (_session is not { } session || _isBusy || _organizing || _dialogOpen
            || _annotationState?.EditingAllowed != true || _selectedAnnotationId is not { } id) return;
        var box = _annotationState.Annotations.LastOrDefault(annotation => annotation.Id == id);
        if (box?.Kind != AnnotationKind.FreeText) return;
        var current = box.Contents ?? string.Empty;

        await PromptFreeTextAsync($"Edit text box — page {box.PageIndex + 1}", "Save", current, "Text box edit canceled.", session,
            text => FreeTextInput.EditChangesText(current, text)
                ? CommitFreeTextAsync(session, new PdfCoreEdit.SetContents(id, text), "Text box edited. Changes are pending save.", selectNew: false)
                : Task.FromResult<string?>(null));
    }

    /// <summary>
    /// Shows the text dialog. <paramref name="commit"/> runs inside the primary
    /// button's deferral: a message back keeps the dialog open with the text as
    /// typed; null closes it. Returns whether the dialog was confirmed.
    /// </summary>
    private async Task<bool> PromptFreeTextAsync(string title, string primaryText, string initial, string canceledStatus,
        DocumentSession session, Func<string, Task<string?>> commit)
    {
        var text = new TextBox
        {
            Header = "Text",
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            Text = initial,
            MinWidth = 300,
            MinHeight = 120,
            MaxHeight = 300,
        };
        var problem = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
            Foreground = (Brush)Application.Current.Resources["SystemFillColorCriticalBrush"],
        };
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = title,
            Content = new StackPanel { Spacing = 8, Children = { text, problem } },
            PrimaryButtonText = primaryText,
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            IsPrimaryButtonEnabled = FreeTextInput.AddEnabled(initial),
        };
        text.TextChanged += (_, _) =>
        {
            dialog.IsPrimaryButtonEnabled = FreeTextInput.AddEnabled(text.Text);
            problem.Visibility = Visibility.Collapsed;
        };
        var committing = false;
        dialog.PrimaryButtonClick += async (_, args) =>
        {
            if (committing)
            {
                args.Cancel = true;
                return;
            }
            committing = true;
            var deferral = args.GetDeferral();
            try
            {
                if (await commit(text.Text) is { } message)
                {
                    problem.Text = message;
                    problem.Visibility = Visibility.Visible;
                    args.Cancel = true;
                }
            }
            finally
            {
                committing = false;
                deferral.Complete();
            }
        };
        dialog.Opened += (_, _) => text.Focus(FocusState.Programmatic);

        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return true;
        if (ImageStampInput.SessionMatches(session.SessionId, _session?.SessionId)) AnnotationStatus.Text = canceledStatus;
        return false;
    }

    /// <summary>
    /// Sends one edit to the core and, when it lands, shows it. Returns the
    /// sentence to put in front of the reader when the core refused (the dialog
    /// stays open), or null when it succeeded or must be dropped quietly
    /// because the document the dialog was opened on is gone.
    /// </summary>
    private async Task<string?> CommitFreeTextAsync(DocumentSession session, PdfCoreEdit edit, string doneStatus, bool selectNew)
    {
        // The dialog awaited the reader: never attach its result to a replacement
        // document, or submit after editing has become unavailable.
        if (!ImageStampInput.SessionMatches(session.SessionId, _session?.SessionId) || _organizing
            || _annotationState?.EditingAllowed != true) return null;

        var before = _annotationState.Annotations;
        var result = await _facade.EditAnnotationAsync(session.SessionId, edit);
        if (!ImageStampInput.SessionMatches(session.SessionId, _session?.SessionId)) return null;
        if (!result.IsSuccess) return result.Error!.Message;

        _annotationState = result.Value!;
        if (selectNew)
        {
            var known = before.Select(annotation => annotation.Id).ToHashSet();
            _selectedAnnotationId = _annotationState.Annotations.LastOrDefault(annotation => !known.Contains(annotation.Id))?.Id;
        }
        AnnotationStatus.Text = doneStatus;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        return null;
    }
}
