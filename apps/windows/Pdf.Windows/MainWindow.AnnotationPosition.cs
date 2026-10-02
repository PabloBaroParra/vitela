using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Collects precise coordinates for the existing core annotation move command.</summary>
public sealed partial class MainWindow
{
    private async void MoveAnnotationButton_Click(object sender, RoutedEventArgs e) => await MoveSelectedAnnotationAsync();

    private async Task MoveSelectedAnnotationAsync()
    {
        if (_session is not { } session || _isBusy || _organizing || _dialogOpen
            || _annotationState?.EditingAllowed != true || _selectedAnnotationId is not { } id) return;
        var selected = _annotationState.Annotations.LastOrDefault(annotation => annotation.Id == id);
        if (selected is null || AnnotationBounds(selected) is not { } bounds) return;

        var x = new NumberBox { Header = "X (pt)", Value = bounds.X };
        var y = new NumberBox { Header = "Y (pt)", Value = bounds.Y };
        var panel = new StackPanel { Spacing = 8, MinWidth = 300 };
        panel.Children.Add(x);
        panel.Children.Add(y);
        panel.Children.Add(new TextBlock
        {
            Text = "Position the annotation's bottom-left bounds in unrotated PDF space. X increases rightward; Y increases upward. Size stays unchanged.",
            TextWrapping = TextWrapping.Wrap,
        });
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = $"Move annotation — page {selected.PageIndex + 1}",
            Content = panel,
            PrimaryButtonText = "Move",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        bool valid() => double.IsFinite(x.Value) && double.IsFinite(y.Value)
            && double.IsFinite(x.Value - bounds.X) && double.IsFinite(y.Value - bounds.Y);
        x.ValueChanged += (_, _) => dialog.IsPrimaryButtonEnabled = valid();
        y.ValueChanged += (_, _) => dialog.IsPrimaryButtonEnabled = valid();
        dialog.IsPrimaryButtonEnabled = valid();
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return;

        // Keep the delta tied to the captured geometry and selection.
        if (_session?.SessionId != session.SessionId || _isBusy || _organizing
            || _annotationState?.EditingAllowed != true || _selectedAnnotationId != id || !valid()) return;
        if (_annotationState.Annotations.LastOrDefault(annotation => annotation.Id == id) != selected) return;
        if (x.Value == bounds.X && y.Value == bounds.Y)
        {
            AnnotationStatus.Text = "Annotation position unchanged.";
            return;
        }

        SetBusy(true);
        try
        {
            var result = await _facade.EditAnnotationAsync(session.SessionId,
                new PdfCoreEdit.Move(id, x.Value - bounds.X, y.Value - bounds.Y));
            if (_session?.SessionId != session.SessionId) return;
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }
            _annotationState = result.Value!;
            AnnotationStatus.Text = "Annotation moved. Changes are pending save.";
            RedrawAnnotations();
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
