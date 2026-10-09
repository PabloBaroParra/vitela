using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;

namespace Pdf.Windows;

/// <summary>Deleting the field selected in Edit forms; availability is set with the inspector in <c>UpdateFormToolbar</c>.</summary>
public sealed partial class MainWindow
{
    private async void DeleteFormFieldButton_Click(object sender, RoutedEventArgs e) => await DeleteSelectedFormFieldAsync();

    /// <summary>
    /// Delete removes the selected field, the same key that removes a selected
    /// annotation. Selecting a field on the page focuses its fill box, and
    /// Delete there edits the value: a text input with focus keeps the key,
    /// and the button stays the way to delete from it.
    /// </summary>
    private async void DeleteFormField_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (TextInputHasFocus())
        {
            args.Handled = false;
            return;
        }

        args.Handled = true;
        await DeleteSelectedFormFieldAsync();
    }

    private async Task DeleteSelectedFormFieldAsync()
    {
        if (_isBusy || _dialogOpen || !DeleteFormFieldButton.IsEnabled || SelectedFormField is not { } field || _session is null) return;
        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            // A fill still on its way to the core must land first, or it would
            // name a field that is no longer there.
            await _formFillTail;
            if (_session?.SessionId != sessionId || _selectedFormFieldId != field.Id) return;
            var result = await _facade.RemoveFormFieldAsync(sessionId, field.Id, field.Name);
            if (result.IsSuccess) SelectFormField(null);
            await ShowFieldStyleResultAsync(sessionId, field, result, "Field deleted. Save to keep the change, or undo to bring it back.");
        }
        finally { SetBusy(false); RedrawAnnotations(); }
    }
}
