using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Form-mode selection and the shared style inspector. Placement and fill commands keep their existing owners.</summary>
public sealed partial class MainWindow
{
    private bool _formEditMode;
    private bool _formStyleSyncing = true;
    private bool _formStylePending;
    private FormFieldState? _formFieldState;
    private ulong? _selectedFormFieldId;
    private FormField? SelectedFormField => _formFieldState?.Fields.FirstOrDefault(field => field.Id == _selectedFormFieldId);

    private void UpdateFormToolbar()
    {
        var enabled = !_isBusy && !_organizing && _session is { PageCount: > 0 } &&
            _formFieldState?.SessionId == _session.SessionId && _formFieldState.StructureAllowed;
        EditFormsButton.IsEnabled = enabled;
        PlaceTextFieldButton.IsEnabled = enabled;
        PlaceCheckboxButton.IsEnabled = enabled;
        PlaceRadioGroupButton.IsEnabled = enabled;
        PlaceDropdownButton.IsEnabled = enabled;
        var style = SelectedFormField?.Style;
        _formStyleSyncing = true;
        try
        {
            if (style is not null)
            {
                FormStyleFont.SelectedIndex = (int)style.Font;
                FormStyleSize.Value = style.SizePt;
            }
            FormStyleFont.IsEnabled = enabled && !_formStylePending && style is not null;
            FormStyleSize.IsEnabled = FormStyleFont.IsEnabled;
            FormStyleColor.IsEnabled = FormStyleFont.IsEnabled;
            DeleteFormFieldButton.IsEnabled = enabled && !_formStylePending && SelectedFormField is not null;
        }
        finally { _formStyleSyncing = false; }
    }

    private async void EditFormsButton_Click(object sender, RoutedEventArgs e)
    {
        var requested = EditFormsButton.IsChecked == true;
        if (!requested)
        {
            StopPlacingFormField();
            RedrawAnnotations();
            FormFieldsStatus.Text = "Edit forms disarmed.";
            return;
        }
        if (!EditFormsButton.IsEnabled)
        {
            EditFormsButton.IsChecked = _formEditMode;
            return;
        }
        var sessionId = _session?.SessionId;
        var generation = _formPlacementGeneration;
        await SettleContentEditorForHistoryAsync();
        if (_session?.SessionId != sessionId || generation != _formPlacementGeneration || !EditFormsButton.IsEnabled) return;
        SetContentEditMode(false);
        _armedAnnotation = null;
        _selectedAnnotationId = null;
        SyncAnnotationToolButtons();
        _textSelection = null;
        _textDragActive = false;
        RedrawSelection();
        _formEditMode = true;
        EditFormsButton.IsChecked = true;
        RedrawAnnotations();
        FormFieldsStatus.Text = "Edit forms armed — pick a field type to place, or click an existing field to select it.";
    }

    private void SelectFormField(ulong? id)
    {
        if (_selectedFormFieldId == id) return;
        _selectedFormFieldId = id;
        UpdateFormToolbar();
        RedrawAnnotations();
    }

    private void DrawFormFieldSelection()
    {
        if (!_formEditMode || SelectedFormField is not { Rect: { } rect } field || field.PageIndex >= _slots.Count) return;
        if (_formGeometryDrag is { } drag && drag.Field.Id == field.Id) rect = FormDragRect(drag);
        var slot = _slots[(int)field.PageIndex];
        var outline = new Rectangle { Stroke = HandleBrush, StrokeThickness = 2, IsHitTestVisible = false };
        PlaceOverPage(outline, slot, (int)field.PageIndex, rect, minSide: 2);
        slot.Annotations.Children.Add(outline);
        DrawHandles(slot, field.PageIndex, rect);
    }

    private async void FormStyleFont_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_formStyleSyncing || !FormStyleFont.IsEnabled || FormStyleFont.SelectedIndex < 0 || SelectedFormField is not { Style: { } style } field) return;
        var font = (FormFont)FormStyleFont.SelectedIndex;
        if (font != style.Font) await ApplyFormInspectorAsync(() => CommitFieldFontAsync(_session!.SessionId, field, style, font));
    }

    private async void FormStyleSize_ValueChanged(NumberBox sender, NumberBoxValueChangedEventArgs e)
    {
        if (_formStyleSyncing || !sender.IsEnabled || !double.IsFinite(e.NewValue) || SelectedFormField is not { Style: { } style } field) return;
        if (e.NewValue != style.SizePt) await ApplyFormInspectorAsync(() => CommitFieldFontSizeAsync(_session!.SessionId, field, style, e.NewValue));
    }

    private async Task ApplyFormInspectorAsync(Func<Task> apply)
    {
        _formStylePending = true;
        UpdateFormToolbar();
        try { await apply(); }
        finally
        {
            _formStylePending = false;
            UpdateFormToolbar();
        }
    }

    private async void FormStyleColor_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _dialogOpen || !FormStyleColor.IsEnabled || SelectedFormField is not { Style: { } style } field || _session is null) return;
        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            var color = await AskColorAsync("Choose field color", style.Color);
            if (color is null || _session?.SessionId != sessionId || _selectedFormFieldId != field.Id) return;
            if (color != style.Color) await CommitFieldColorAsync(sessionId, field, style, color);
        }
        finally { SetBusy(false); RestoreAnnotationControls(); }
    }
}
