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
        await SaveFromShellAsync();
    }

    private void ShellPrint_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs e)
    {
        e.Handled = true;
        PrintFromShell();
    }

    /// <summary>Ctrl+S and the palette's Save: the Save button's flow unless the shell is busy or Save is off. Returns whether it ran.</summary>
    private async Task<bool> SaveFromShellAsync()
    {
        if (_isBusy || _dialogOpen || _shellPickingFile || !SaveButton.IsEnabled) return false;
        await SaveToPickedFileAsync();
        return true;
    }

    /// <summary>Ctrl+P and the palette's Print: the Print button's flow unless the shell is busy or no page is open. Returns whether it ran.</summary>
    private bool PrintFromShell()
    {
        if (_isBusy || _dialogOpen || _shellPickingFile || _session is not { PageCount: > 0 }) return false;
        PrintButton_Click(PrintButton, new RoutedEventArgs());
        return true;
    }
}
