using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Presents core-owned note contents without recording an edit.</summary>
public sealed partial class MainWindow
{
    private async void ReadNoteButton_Click(object sender, RoutedEventArgs e) => await ReadSelectedNoteAsync();

    private async Task ReadSelectedNoteAsync()
    {
        if (_session is null || _isBusy || _organizing || _dialogOpen || _selectedAnnotationId is not { } id) return;
        var note = _annotationState?.Annotations.LastOrDefault(annotation => annotation.Id == id);
        if (note?.Kind != AnnotationKind.TextNote) return;

        var text = new TextBox
        {
            Header = "Note text",
            IsReadOnly = true,
            AcceptsReturn = true,
            Text = note.Contents ?? string.Empty,
            TextWrapping = TextWrapping.Wrap,
            MinWidth = 300,
            MinHeight = 120,
            MaxHeight = 300,
        };
        var dialog = new ContentDialog
        {
            XamlRoot = PageScroller.XamlRoot,
            Title = $"Note — page {note.PageIndex + 1}",
            Content = text,
            CloseButtonText = "Close",
            DefaultButton = ContentDialogButton.Close,
        };
        dialog.Opened += (_, _) => text.Focus(FocusState.Programmatic);
        await ShowModalAsync(dialog);
    }
}
