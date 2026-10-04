using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;

namespace Pdf.Windows;

/// <summary>Shell-wide file commands must not depend on the editor being visible.</summary>
public sealed partial class MainWindow
{
    private async void ShellOpen_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs e)
    {
        e.Handled = true;
        await PickShellDocumentAsync();
    }

    private async void ShellSave_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs e)
    {
        e.Handled = true;
        if (!_isBusy && !_dialogOpen && !_shellPickingFile && SaveButton.IsEnabled)
            await SaveToPickedFileAsync();
    }

    private void ShellPrint_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs e)
    {
        e.Handled = true;
        if (!_isBusy && !_dialogOpen && !_shellPickingFile && _session is { PageCount: > 0 })
            PrintButton_Click(PrintButton, new RoutedEventArgs());
    }
}
