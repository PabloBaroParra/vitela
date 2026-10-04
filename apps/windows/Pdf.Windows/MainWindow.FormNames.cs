using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Windows.System;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    // A native read-only TextBox becomes a name editor on double click or F2.
    // Ending editing commits once; Escape restores the original without a command.
    private TextBox FormFieldName(string sessionId, FormField field, bool enabled)
    {
        var generation = _formRowsGeneration;
        var box = new TextBox { Text = field.Name, IsReadOnly = true, IsEnabled = enabled };
        AutomationProperties.SetName(box, $"Field name: {field.Name}");
        ToolTipService.SetToolTip(box, "Double-click or press F2 to rename. Enter accepts; Escape cancels.");
        var editing = false;
        void begin()
        {
            if (!box.IsEnabled || _isBusy || generation != _formRowsGeneration) return;
            editing = true;
            box.IsReadOnly = false;
            box.Focus(FocusState.Programmatic);
            box.SelectAll();
        }
        async Task finish(bool accept)
        {
            if (!editing) return;
            editing = false;
            box.IsReadOnly = true;
            var name = box.Text;
            if (!accept || _isBusy || generation != _formRowsGeneration) box.Text = field.Name;
            else if (generation == _formRowsGeneration && !_isBusy)
            {
                await _formFillTail;
                if (generation == _formRowsGeneration) await CommitFieldNameAsync(sessionId, field, name);
            }
        }
        box.DoubleTapped += (_, args) => { begin(); args.Handled = true; };
        box.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.F2) { begin(); args.Handled = true; }
            else if (editing && args.Key is VirtualKey.Enter or VirtualKey.Escape)
            {
                args.Handled = true;
                await finish(args.Key == VirtualKey.Enter);
            }
        };
        box.LostFocus += async (_, _) => await finish(true);
        return box;
    }
}
