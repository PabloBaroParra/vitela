using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Windows.System;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    // The name is a plain editable TextBox: click and type. Enter or leaving the
    // box commits once; Escape restores the original without a command. The
    // commit rebuilds the rows, and the old box's LostFocus must not resend it.
    private TextBox FormFieldName(string sessionId, FormField field, bool enabled)
    {
        var generation = _formRowsGeneration;
        var box = new TextBox { Text = field.Name, IsEnabled = enabled };
        AutomationProperties.SetName(box, $"Field name: {field.Name}");
        ToolTipService.SetToolTip(box, "Type to rename. Enter accepts; Escape cancels.");
        var submitted = false;
        async Task commit()
        {
            if (submitted || box.Text == field.Name) return;
            if (_isBusy || generation != _formRowsGeneration)
            {
                box.Text = field.Name;
                return;
            }
            submitted = true;
            var name = box.Text;
            await _formFillTail;
            if (generation == _formRowsGeneration) await CommitFieldNameAsync(sessionId, field, name);
        }
        box.KeyDown += async (_, args) =>
        {
            if (args.Key == VirtualKey.Escape)
            {
                args.Handled = true;
                box.Text = field.Name;
                box.SelectAll();
            }
            else if (args.Key == VirtualKey.Enter)
            {
                args.Handled = true;
                await commit();
            }
        };
        box.LostFocus += async (_, _) => await commit();
        return box;
    }
}
