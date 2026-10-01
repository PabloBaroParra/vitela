using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Collects note contents before the core records the placement.</summary>
public sealed partial class MainWindow
{
    private async Task PlaceTextNoteAsync(uint pageIndex, PdfCoreRect rect)
    {
        if (_session is not { } session || _isBusy || _organizing || _annotationState?.EditingAllowed != true) return;
        var text = new TextBox
        {
            Header = "Note text",
            AcceptsReturn = true,
            TextWrapping = TextWrapping.Wrap,
            MinWidth = 300,
            MinHeight = 120,
            MaxHeight = 300,
        };
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = $"Add note — page {pageIndex + 1}",
            Content = text,
            PrimaryButtonText = "Add",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            IsPrimaryButtonEnabled = false,
        };
        text.TextChanged += (_, _) => dialog.IsPrimaryButtonEnabled = !string.IsNullOrWhiteSpace(text.Text);
        dialog.Opened += (_, _) => text.Focus(FocusState.Programmatic);
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary)
        {
            if (_session?.SessionId == session.SessionId) AnnotationStatus.Text = "Note placement canceled.";
            return;
        }
        // The prompt awaits user input: never attach the old page rectangle to
        // a replacement session, or submit after editing has become unavailable.
        if (_session?.SessionId != session.SessionId || _isBusy || _organizing
            || _annotationState?.EditingAllowed != true || string.IsNullOrWhiteSpace(text.Text)) return;

        SetBusy(true);
        try
        {
            await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.TextNote, pageIndex, rect,
                new PdfCoreColor(DefaultAnnotationColor.R, DefaultAnnotationColor.G, DefaultAnnotationColor.B), Contents: text.Text));
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
