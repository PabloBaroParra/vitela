using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Collects precise dimensions for the existing core annotation resize command.</summary>
public sealed partial class MainWindow
{
    private async void ResizeAnnotationButton_Click(object sender, RoutedEventArgs e) => await ResizeSelectedAnnotationAsync();

    private async Task ResizeSelectedAnnotationAsync()
    {
        if (_session is not { } session || _isBusy || _organizing || _dialogOpen
            || _annotationState?.EditingAllowed != true || _selectedAnnotationId is not { } id) return;
        var selected = _annotationState.Annotations.LastOrDefault(annotation => annotation.Id == id);
        if (selected?.Rect is not { } rect) return;

        var width = new NumberBox { Header = "Width (pt)", Value = rect.Width };
        var height = new NumberBox { Header = "Height (pt)", Value = rect.Height };
        var panel = new StackPanel { Spacing = 8, MinWidth = 300 };
        panel.Children.Add(width);
        panel.Children.Add(height);
        panel.Children.Add(new TextBlock
        {
            Text = "The annotation stays at its current PDF-space origin. Width and height are independent; stamps stretch to fit.",
            TextWrapping = TextWrapping.Wrap,
        });
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = $"Resize annotation — page {selected.PageIndex + 1}",
            Content = panel,
            PrimaryButtonText = "Resize",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        bool valid() => double.IsFinite(width.Value) && width.Value > 0
            && double.IsFinite(height.Value) && height.Value > 0;
        width.ValueChanged += (_, _) => dialog.IsPrimaryButtonEnabled = valid();
        height.ValueChanged += (_, _) => dialog.IsPrimaryButtonEnabled = valid();
        dialog.IsPrimaryButtonEnabled = valid();
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return;

        // The dialog captured a snapshot: do not resize a replaced session or
        // overwrite an annotation changed while the prompt awaited input.
        if (_session?.SessionId != session.SessionId || _isBusy || _organizing
            || _annotationState?.EditingAllowed != true || _selectedAnnotationId != id || !valid()) return;
        if (_annotationState.Annotations.LastOrDefault(annotation => annotation.Id == id) != selected) return;
        if (width.Value == rect.Width && height.Value == rect.Height)
        {
            AnnotationStatus.Text = "Annotation dimensions unchanged.";
            return;
        }

        SetBusy(true);
        try
        {
            var result = await _facade.EditAnnotationAsync(session.SessionId,
                new PdfCoreEdit.Resize(id, new PdfCoreRect(rect.X, rect.Y, width.Value, height.Value)));
            if (_session?.SessionId != session.SessionId) return;
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _annotationState = result.Value!;
            AnnotationStatus.Text = "Annotation resized. Changes are pending save.";
            RedrawAnnotations();
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
