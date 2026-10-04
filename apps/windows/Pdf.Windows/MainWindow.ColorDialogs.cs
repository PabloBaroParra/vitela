using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>RGB-only native color selection; cancellation never mutates a target.</summary>
public sealed partial class MainWindow
{
    private async Task<AnnotationColor?> AskColorAsync(string title, AnnotationColor initial)
    {
        var picker = new ColorPicker
        {
            IsAlphaEnabled = false,
            Color = global::Windows.UI.Color.FromArgb(255, initial.R, initial.G, initial.B),
        };
        var dialog = new ContentDialog
        {
            XamlRoot = Content.XamlRoot, Title = title, Content = picker,
            PrimaryButtonText = "Choose", CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return null;
        return new AnnotationColor(picker.Color.R, picker.Color.G, picker.Color.B);
    }

    private async void AnnotationColorButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || !AnnotationColorButton.IsEnabled || _session is not { } session
            || _selectedAnnotationId is not { } id) return;
        var before = _annotationState?.Annotations.SingleOrDefault(a => a.Id == id);
        if (before?.Color is not { } initial || !SupportsRestyle(before.Kind)) return;
        SetBusy(true);
        try
        {
            var chosen = await AskColorAsync("Choose annotation color", initial);
            if (chosen is null || chosen == initial || _session?.SessionId != session.SessionId
                || _selectedAnnotationId != id) return;
            var result = await _facade.RestyleAnnotationAsync(session.SessionId, before, chosen);
            if (_session?.SessionId != session.SessionId) return;
            if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
            _annotationState = result.Value!;
            AnnotationStatus.Text = "Annotation restyled. Changes are pending save.";
            RedrawAnnotations();
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
