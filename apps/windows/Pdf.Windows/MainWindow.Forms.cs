using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Form fields: filling the AcroForm fields a document already has.
/// </summary>
/// <remarks>
/// <para>
/// One control per field, built from the core's list and never from the
/// page: the panel is the whole fill surface, as it is in the GTK shell.
/// Each control is built so it cannot <em>offer</em> an invalid value — a
/// capped length, a list of the real options — and the core still validates
/// what arrives.
/// </para>
/// <para>
/// A commit never rebuilds the rows. Rebuilding would replace the control the
/// reader is typing in and drop their focus mid-field; the rows are rebuilt
/// only when the fields changed from outside the panel (a new document, an
/// undo or redo, a refused fill that has to show the core's value again).
/// </para>
/// <para>
/// Nothing is drawn over the page: pdfium paints a field's value from its
/// appearance, and the facade rebuilds the preview after every fill, so the
/// page re-renders showing it.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>
    /// Every page this session has filled a field on. Grows only, for the
    /// same reason <see cref="_contentEditedPages"/> does: an undo has to
    /// re-render the page to show the old value coming back.
    /// </summary>
    private readonly HashSet<uint> _filledFieldPages = [];

    /// <summary>
    /// The value each row last showed or committed, so leaving a field
    /// untouched never costs a round trip to the core.
    /// </summary>
    private readonly Dictionary<ulong, FormFieldValue> _shownFieldValues = [];
    private int _formRowsGeneration;
    private Task _formFillTail = Task.CompletedTask;
    private readonly Dictionary<ulong, Control> _formFocusTargets = [];

    private void ResetFormFieldState()
    {
        StopPlacingFormField();
        _formFieldState = null;
        UpdateFormToolbar();
        _filledFieldPages.Clear();
        _shownFieldValues.Clear();
        _formRowsGeneration++;
        _formFocusTargets.Clear();
        FormFieldRows.Children.Clear();
        FormFieldsStatus.Text = "Open a PDF with form fields to fill them in, or place one in Edit forms mode.";
    }

    private async Task RefreshFormFieldsAsync()
    {
        if (_session is null) return;

        var sessionId = _session.SessionId;
        var generation = _formRowsGeneration;
        var result = await _facade.FormFieldsAsync(sessionId);
        if (_session?.SessionId != sessionId || generation != _formRowsGeneration) return;

        if (!result.IsSuccess)
        {
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        ShowFormFields(result.Value!);
    }

    private void ShowFormFields(FormFieldState state)
    {
        _formFieldState = state;
        if (!state.Fields.Any(field => field.Id == _selectedFormFieldId)) _selectedFormFieldId = null;
        UpdateFormToolbar();
        _formRowsGeneration++;
        _formFocusTargets.Clear();
        FormFieldRows.Children.Clear();
        _shownFieldValues.Clear();
        FormFieldsStatus.Text = state.Fields.Count == 0
            ? "Open a PDF with form fields to fill them in, or place one in Edit forms mode."
            : state.FillAllowed ? "" : "This document does not permit filling in its form.";

        foreach (var field in state.Fields)
        {
            _shownFieldValues[field.Id] = field.Value;
            var value = FormFieldRow(state.SessionId, field);
            AutomationProperties.SetName(value, field.Name);
            if (!state.FillAllowed) DisableRow(value);
            var row = new StackPanel { Spacing = 4 };
            row.GotFocus += (_, _) =>
            {
                if (_formEditMode) SelectFormField(field.Id);
            };
            row.Children.Add(FormFieldName(state.SessionId, field, state.FillAllowed));
            row.Children.Add(value);
            if (state.StructureAllowed)
            {
                if (field.Rect is { } rect)
                {
                    var position = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                    var x = new NumberBox { Header = "X (pt)", Value = rect.X, Width = 110 };
                    var y = new NumberBox { Header = "Y (pt)", Value = rect.Y, Width = 110 };
                    var positionSubmitted = false;
                    async Task commit()
                    {
                        if (positionSubmitted || (x.Value == rect.X && y.Value == rect.Y)) return;
                        positionSubmitted = true;
                        await CommitFieldPositionAsync(state.SessionId, field, rect, x, y);
                    }
                    x.LostFocus += async (_, _) => await commit();
                    y.LostFocus += async (_, _) => await commit();
                    position.Children.Add(x);
                    position.Children.Add(y);
                    row.Children.Add(position);
                    var dimensions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 8 };
                    var width = new NumberBox { Header = "Width (pt)", Value = rect.Width, Width = 110 };
                    var height = new NumberBox { Header = "Height (pt)", Value = rect.Height, Width = 110 };
                    var sizeSubmitted = false;
                    async Task commitSize()
                    {
                        if (sizeSubmitted || (width.Value == rect.Width && height.Value == rect.Height)) return;
                        sizeSubmitted = true;
                        await CommitFieldSizeAsync(state.SessionId, field, rect, width.Value, height.Value);
                    }
                    width.LostFocus += async (_, _) => await commitSize();
                    height.LostFocus += async (_, _) => await commitSize();
                    dimensions.Children.Add(width);
                    dimensions.Children.Add(height);
                    row.Children.Add(dimensions);
                }
            }
            FormFieldRows.Children.Add(row);
        }
    }

    private async Task CommitFieldNameAsync(string sessionId, FormField field, string name)
    {
        if (_session?.SessionId != sessionId || name == field.Name) return;
        var result = await _facade.RenameFormFieldAsync(sessionId, field.Id, field.Name, name);
        if (_session?.SessionId != sessionId) return;
        if (!result.IsSuccess)
        {
            await RefreshFormFieldsAsync();
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        _filledFieldPages.Add(field.PageIndex);
        InvalidatePageRender(field.PageIndex);
        await RefreshFormFieldsAsync();
        FormFieldsStatus.Text = "Field renamed. Save to keep the change.";
    }

    private async Task CommitFieldPositionAsync(string sessionId, FormField field, AnnotationRect rect, NumberBox x, NumberBox y)
    {
        if (_session?.SessionId != sessionId || (x.Value == rect.X && y.Value == rect.Y)) return;
        var result = await _facade.MoveFormFieldAsync(sessionId, field.Id, rect, x.Value, y.Value);
        await ShowFieldStyleResultAsync(sessionId, field, result, "Field moved. Save to keep the change.");
    }

    private async Task CommitFieldSizeAsync(string sessionId, FormField field, AnnotationRect rect, double width, double height)
    {
        if (_session?.SessionId != sessionId) return;
        var result = await _facade.ResizeFormFieldAsync(sessionId, field.Id, rect, width, height);
        await ShowFieldStyleResultAsync(sessionId, field, result, "Field resized. Save to keep the change.");
    }

    private async Task CommitFieldFontSizeAsync(string sessionId, FormField field, FormTextStyle style, double sizePt)
    {
        if (_session?.SessionId != sessionId) return;
        var result = await _facade.SetFormFieldFontSizeAsync(sessionId, field.Id, style, sizePt);
        await ShowFieldStyleResultAsync(sessionId, field, result, "Field font size changed. Save to keep the change.");
    }

    private async Task CommitFieldFontAsync(string sessionId, FormField field, FormTextStyle style, FormFont font)
    {
        if (_session?.SessionId != sessionId) return;
        var result = await _facade.SetFormFieldFontAsync(sessionId, field.Id, style, font);
        await ShowFieldStyleResultAsync(sessionId, field, result, "Field font changed. Save to keep the change.");
    }

    private async Task CommitFieldColorAsync(string sessionId, FormField field, FormTextStyle style, AnnotationColor color)
    {
        if (_session?.SessionId != sessionId) return;
        var result = await _facade.SetFormFieldColorAsync(sessionId, field.Id, style, color);
        await ShowFieldStyleResultAsync(sessionId, field, result, "Field text color changed. Save to keep the change.");
    }

    private async Task ShowFieldStyleResultAsync(string sessionId, FormField field, OperationResult<AnnotationState> result, string message)
    {
        if (_session?.SessionId != sessionId) return;
        if (!result.IsSuccess)
        {
            await RefreshFormFieldsAsync();
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        _filledFieldPages.Add(field.PageIndex);
        InvalidatePageRender(field.PageIndex);
        await RefreshFormFieldsAsync();
        FormFieldsStatus.Text = message;
    }

    private FrameworkElement FormFieldRow(string sessionId, FormField field) => field.Kind switch
    {
        FormFieldKind.Text text => TextFieldRow(sessionId, field, text),
        FormFieldKind.Checkbox => CheckboxRow(sessionId, field),
        FormFieldKind.RadioGroup radio => RadioGroupRow(sessionId, field, radio),
        FormFieldKind.Dropdown { Editable: true } => EditableDropdownRow(sessionId, field),
        FormFieldKind.Dropdown dropdown => DropdownRow(sessionId, field, dropdown),
        _ => new TextBlock { Text = "Unsupported field", TextWrapping = TextWrapping.Wrap },
    };

    private TextBox TextFieldRow(string sessionId, FormField field, FormFieldKind.Text kind)
    {
        var box = new TextBox
        {
            Text = field.Value is FormFieldValue.Text text ? FormFieldText.ToTextBox(text.Value) : "",
            AcceptsReturn = kind.Multiline,
            TextWrapping = kind.Multiline ? TextWrapping.Wrap : TextWrapping.NoWrap,
        };
        // WinUI reads MaxLength 0 as unlimited; preserve a real zero-length limit.
        if (kind.MaxLength == 0) box.IsReadOnly = true;
        else if (kind.MaxLength is { } max) box.MaxLength = (int)Math.Min(max, int.MaxValue);

        var generation = _formRowsGeneration;
        box.TextChanging += async (_, _) =>
        {
            if (box.IsEnabled && !box.IsReadOnly)
                await QueueFormFillAsync(sessionId, field, generation, new FormFieldValue.Text(FormFieldText.FromTextBox(box.Text)));
        };
        _formFocusTargets[field.Id] = box;
        return box;
    }

    private CheckBox CheckboxRow(string sessionId, FormField field)
    {
        var check = new CheckBox
        {
            IsChecked = field.Value is FormFieldValue.Checked { Value: true },
        };
        // Click, not Checked/Unchecked: it fires only for the reader, never for
        // the initial state set just above.
        var generation = _formRowsGeneration;
        check.Click += async (_, _) =>
            await QueueFormFillAsync(sessionId, field, generation, new FormFieldValue.Checked(check.IsChecked == true));
        _formFocusTargets[field.Id] = check;
        return check;
    }

    private StackPanel RadioGroupRow(string sessionId, FormField field, FormFieldKind.RadioGroup kind)
    {
        var chosen = field.Value is FormFieldValue.Choice choice ? choice.Option : null;
        var group = new StackPanel { Spacing = 2 };
        var generation = _formRowsGeneration;
        foreach (var option in kind.Options)
        {
            var button = new RadioButton
            {
                Content = option,
                // One group per field: WinUI groups by name across the whole
                // window, so a shared name would link unrelated fields.
                GroupName = $"form-field-{field.Id}",
                IsChecked = option == chosen,
            };
            button.Checked += async (_, _) => await QueueFormFillAsync(sessionId, field, generation, new FormFieldValue.Choice(option));
            if (!_formFocusTargets.ContainsKey(field.Id)) _formFocusTargets[field.Id] = button;
            group.Children.Add(button);
        }

        return group;
    }

    private ComboBox DropdownRow(string sessionId, FormField field, FormFieldKind.Dropdown kind)
    {
        var combo = new ComboBox { HorizontalAlignment = HorizontalAlignment.Stretch };
        combo.Items.Add(FormFieldChoices.NoChoiceLabel);
        foreach (var option in kind.Options) combo.Items.Add(option);
        combo.SelectedIndex = FormFieldChoices.IndexFor(kind.Options, field.Value is FormFieldValue.Choice choice ? choice.Option : null);
        // Subscribed after the initial selection, which would otherwise arrive
        // as a commit of the value the field already holds.
        var generation = _formRowsGeneration;
        combo.SelectionChanged += async (_, _) =>
            await QueueFormFillAsync(sessionId, field, generation, new FormFieldValue.Choice(FormFieldChoices.ChoiceFor(kind.Options, combo.SelectedIndex)));
        _formFocusTargets[field.Id] = combo;
        return combo;
    }

    /// <summary>
    /// An editable dropdown accepts text outside its options, so it is typed
    /// into like a text field — the GTK shell's choice too. The options are
    /// still named, as the placeholder.
    /// </summary>
    private TextBox EditableDropdownRow(string sessionId, FormField field)
    {
        var options = field.Kind is FormFieldKind.Dropdown dropdown ? dropdown.Options : [];
        var box = new TextBox
        {
            Text = field.Value is FormFieldValue.Choice { Option: { } option } ? option : "",
            PlaceholderText = string.Join(", ", options),
        };
        var generation = _formRowsGeneration;
        box.TextChanging += async (_, _) =>
        {
            if (box.IsEnabled) await QueueFormFillAsync(sessionId, field, generation, EditableChoice(box.Text));
        };
        _formFocusTargets[field.Id] = box;
        return box;
    }

    private static FormFieldValue EditableChoice(string text) =>
        new FormFieldValue.Choice(text.Length == 0 ? null : text);

    private static void DisableRow(FrameworkElement row)
    {
        if (row is Control control)
        {
            control.IsEnabled = false;
            return;
        }

        if (row is Panel panel)
        {
            foreach (var child in panel.Children.OfType<Control>()) child.IsEnabled = false;
        }
    }

    /// <summary>
    /// Fills <paramref name="field"/> in, for the session its row was built
    /// for. A row can still commit after a new document replaced it — a
    /// TextBox losing focus as the panel is cleared — and its field id means
    /// nothing against the new bytes, so that commit is dropped.
    /// </summary>
    private Task QueueFormFillAsync(string sessionId, FormField field, int generation, FormFieldValue value)
    {
        if (_session?.SessionId != sessionId || generation != _formRowsGeneration || _isBusy || _formFieldState?.FillAllowed != true)
            return Task.CompletedTask;
        if (_shownFieldValues.TryGetValue(field.Id, out var shown) && shown == value) return Task.CompletedTask;
        _shownFieldValues[field.Id] = value;
        // Submit immediately: the facade's document-change gate orders these
        // writes before a subsequent Save/Undo, even while preview I/O awaits.
        var commit = CommitFormFillAsync(sessionId, field, generation, value);
        _formFillTail = _formFillTail.IsCompleted ? commit : Task.WhenAll(_formFillTail, commit);
        return commit;
    }

    private async Task CommitFormFillAsync(string sessionId, FormField field, int generation, FormFieldValue value)
    {
        var result = await _facade.SetFormFieldValueAsync(sessionId, field.Id, value);
        // TextChanging is synchronous and may occur during layout. Never
        // rebuild rows or update sibling controls inside that event's stack.
        await Task.Yield();
        if (_session?.SessionId != sessionId || generation != _formRowsGeneration) return;

        if (!result.IsSuccess)
        {
            // Put the rows back to what the core holds, then say why: the
            // rebuild clears the status line, so the message goes last.
            await RefreshFormFieldsAsync();
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        _annotationState = result.Value;
        if (_formFieldState is { } state)
            _formFieldState = state with { Fields = state.Fields.Select(candidate => candidate.Id == field.Id ? candidate with { Value = value } : candidate).ToArray() };
        UpdateAnnotationControls(_annotationState);
        _filledFieldPages.Add(field.PageIndex);
        InvalidatePageRender(field.PageIndex);
        FormFieldsStatus.Text = "Field filled in. Save to keep the change.";
    }

    /// <summary>
    /// Re-renders every page a field was filled on — what an undo or redo
    /// needs, since neither says which field it moved.
    /// </summary>
    private void InvalidateFilledFieldPages()
    {
        foreach (var pageIndex in _filledFieldPages)
        {
            if (pageIndex < _slots.Count)
            {
                _slots[(int)pageIndex].Render.DropBitmap();
                _slots[(int)pageIndex].Tiles.Clear();
            }
        }

        UpdateViewport(intermediate: false);
    }
}
