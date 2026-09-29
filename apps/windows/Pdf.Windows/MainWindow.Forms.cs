using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;

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

    private void ResetFormFieldState()
    {
        StopPlacingFormField();
        _filledFieldPages.Clear();
        _shownFieldValues.Clear();
        FormFieldRows.Children.Clear();
        FormFieldsStatus.Text = "";
    }

    private async Task RefreshFormFieldsAsync()
    {
        if (_session is null) return;

        var sessionId = _session.SessionId;
        var result = await _facade.FormFieldsAsync(sessionId);
        if (_session?.SessionId != sessionId) return;

        if (!result.IsSuccess)
        {
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        ShowFormFields(result.Value!);
    }

    private void ShowFormFields(FormFieldState state)
    {
        PlaceTextFieldButton.IsEnabled = state.FillAllowed && _session?.ContentEditingAllowed == true;
        PlaceCheckboxButton.IsEnabled = PlaceTextFieldButton.IsEnabled;
        PlaceRadioGroupButton.IsEnabled = PlaceTextFieldButton.IsEnabled;
        PlaceDropdownButton.IsEnabled = PlaceTextFieldButton.IsEnabled;
        FormFieldRows.Children.Clear();
        _shownFieldValues.Clear();
        FormFieldsStatus.Text = state.Fields.Count == 0
            ? "This document has no form fields."
            : state.FillAllowed ? "" : "This document does not permit filling in its form.";

        foreach (var field in state.Fields)
        {
            _shownFieldValues[field.Id] = field.Value;
            var row = FormFieldRow(state.SessionId, field);
            if (!state.FillAllowed) DisableRow(row);
            FormFieldRows.Children.Add(row);
        }
    }

    private FrameworkElement FormFieldRow(string sessionId, FormField field) => field.Kind switch
    {
        FormFieldKind.Text text => TextFieldRow(sessionId, field, text),
        FormFieldKind.Checkbox => CheckboxRow(sessionId, field),
        FormFieldKind.RadioGroup radio => RadioGroupRow(sessionId, field, radio),
        FormFieldKind.Dropdown { Editable: true } => EditableDropdownRow(sessionId, field),
        FormFieldKind.Dropdown dropdown => DropdownRow(sessionId, field, dropdown),
        _ => new TextBlock { Text = field.Name },
    };

    private TextBox TextFieldRow(string sessionId, FormField field, FormFieldKind.Text kind)
    {
        var box = new TextBox
        {
            Header = field.Name,
            Text = field.Value is FormFieldValue.Text text ? FormFieldText.ToTextBox(text.Value) : "",
            AcceptsReturn = kind.Multiline,
            TextWrapping = kind.Multiline ? TextWrapping.Wrap : TextWrapping.NoWrap,
        };
        // WinUI reads MaxLength 0 as "no limit", while the core sends 0 for a
        // field nobody may type into — a kind it does not model — so that one
        // becomes read-only instead of unlimited.
        if (kind.MaxLength == 0) box.IsReadOnly = true;
        else if (kind.MaxLength is { } max) box.MaxLength = (int)Math.Min(max, int.MaxValue);

        box.LostFocus += async (_, _) =>
            await CommitFormFieldAsync(sessionId, field, new FormFieldValue.Text(FormFieldText.FromTextBox(box.Text)));
        if (!kind.Multiline) CommitOnEnter(box, sessionId, field, () => new FormFieldValue.Text(box.Text));
        return box;
    }

    private CheckBox CheckboxRow(string sessionId, FormField field)
    {
        var check = new CheckBox
        {
            Content = field.Name,
            IsChecked = field.Value is FormFieldValue.Checked { Value: true },
        };
        // Click, not Checked/Unchecked: it fires only for the reader, never for
        // the initial state set just above.
        check.Click += async (_, _) =>
            await CommitFormFieldAsync(sessionId, field, new FormFieldValue.Checked(check.IsChecked == true));
        return check;
    }

    private StackPanel RadioGroupRow(string sessionId, FormField field, FormFieldKind.RadioGroup kind)
    {
        var chosen = field.Value is FormFieldValue.Choice choice ? choice.Option : null;
        var group = new StackPanel { Spacing = 2 };
        group.Children.Add(new TextBlock { Text = field.Name });
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
            button.Click += async (_, _) => await CommitFormFieldAsync(sessionId, field, new FormFieldValue.Choice(option));
            group.Children.Add(button);
        }

        return group;
    }

    private ComboBox DropdownRow(string sessionId, FormField field, FormFieldKind.Dropdown kind)
    {
        var combo = new ComboBox { Header = field.Name, HorizontalAlignment = HorizontalAlignment.Stretch };
        combo.Items.Add(FormFieldChoices.NoChoiceLabel);
        foreach (var option in kind.Options) combo.Items.Add(option);
        combo.SelectedIndex = FormFieldChoices.IndexFor(kind.Options, field.Value is FormFieldValue.Choice choice ? choice.Option : null);
        // Subscribed after the initial selection, which would otherwise arrive
        // as a commit of the value the field already holds.
        combo.SelectionChanged += async (_, _) =>
            await CommitFormFieldAsync(sessionId, field, new FormFieldValue.Choice(FormFieldChoices.ChoiceFor(kind.Options, combo.SelectedIndex)));
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
            Header = field.Name,
            Text = field.Value is FormFieldValue.Choice { Option: { } option } ? option : "",
            PlaceholderText = string.Join(", ", options),
        };
        box.LostFocus += async (_, _) => await CommitFormFieldAsync(sessionId, field, EditableChoice(box.Text));
        CommitOnEnter(box, sessionId, field, () => EditableChoice(box.Text));
        return box;
    }

    private static FormFieldValue EditableChoice(string text) =>
        new FormFieldValue.Choice(text.Length == 0 ? null : text);

    private void CommitOnEnter(TextBox box, string sessionId, FormField field, Func<FormFieldValue> value)
    {
        box.KeyDown += async (_, args) =>
        {
            if (args.Key != VirtualKey.Enter) return;
            args.Handled = true;
            await CommitFormFieldAsync(sessionId, field, value());
        };
    }

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
    private async Task CommitFormFieldAsync(string sessionId, FormField field, FormFieldValue value)
    {
        if (_session?.SessionId != sessionId) return;
        if (_shownFieldValues.TryGetValue(field.Id, out var shown) && shown == value) return;

        _shownFieldValues[field.Id] = value;
        var result = await _facade.SetFormFieldValueAsync(sessionId, field.Id, value);
        if (_session?.SessionId != sessionId) return;

        if (!result.IsSuccess)
        {
            // Put the rows back to what the core holds, then say why: the
            // rebuild clears the status line, so the message goes last.
            await RefreshFormFieldsAsync();
            FormFieldsStatus.Text = result.Error!.Message;
            return;
        }

        _annotationState = result.Value;
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
