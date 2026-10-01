using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Placing a form field on a page; the core owns its identity and undo step.</summary>
public sealed partial class MainWindow
{
    private enum FieldToPlace { Text, Checkbox, RadioGroup, Dropdown }

    private FieldToPlace? _placingFormField;
    private bool _creatingFormField;
    private (int PageIndex, AnnotationPoint Origin)? _formFieldPress;
    private uint _formPlacementGeneration;

    private async void PlaceTextFieldButton_Click(object sender, RoutedEventArgs e) =>
        await SetFormFieldPlacementAsync(FieldToPlace.Text, PlaceTextFieldButton.IsChecked == true);

    private async void PlaceCheckboxButton_Click(object sender, RoutedEventArgs e) =>
        await SetFormFieldPlacementAsync(FieldToPlace.Checkbox, PlaceCheckboxButton.IsChecked == true);

    private async void PlaceRadioGroupButton_Click(object sender, RoutedEventArgs e) =>
        await SetFormFieldPlacementAsync(FieldToPlace.RadioGroup, PlaceRadioGroupButton.IsChecked == true);

    private async void PlaceDropdownButton_Click(object sender, RoutedEventArgs e) =>
        await SetFormFieldPlacementAsync(FieldToPlace.Dropdown, PlaceDropdownButton.IsChecked == true);

    private async Task SetFormFieldPlacementAsync(FieldToPlace kind, bool requested)
    {
        StopPlacingFormField();
        if (!requested) return;
        var button = kind switch
        {
            FieldToPlace.Text => PlaceTextFieldButton,
            FieldToPlace.Checkbox => PlaceCheckboxButton,
            FieldToPlace.RadioGroup => PlaceRadioGroupButton,
            _ => PlaceDropdownButton,
        };
        button.IsChecked = true;
        var generation = _formPlacementGeneration;
        var sessionId = _session?.SessionId;
        if (sessionId is null)
        {
            StopPlacingFormField();
            return;
        }
        await SettleContentEditorForHistoryAsync();
        if (_session?.SessionId != sessionId || generation != _formPlacementGeneration || button.IsChecked != true)
        {
            if (generation == _formPlacementGeneration) StopPlacingFormField();
            return;
        }
        SetContentEditMode(false);
        _placingFormField = kind;
        _armedAnnotation = null;
        SyncAnnotationToolButtons();
        FormFieldsPanel.IsExpanded = true;
        FormFieldsStatus.Text = kind switch
        {
            FieldToPlace.Text => "Click or drag on a page to place a text field.",
            FieldToPlace.Checkbox => "Click or drag on a page to place a checkbox.",
            FieldToPlace.RadioGroup => "Click or drag on a page to place a radio group.",
            _ => "Click or drag on a page to place a dropdown.",
        };
    }

    private void StopPlacingFormField()
    {
        _formPlacementGeneration++;
        _placingFormField = null;
        _formFieldPress = null;
        PlaceTextFieldButton.IsChecked = false;
        PlaceCheckboxButton.IsChecked = false;
        PlaceRadioGroupButton.IsChecked = false;
        PlaceDropdownButton.IsChecked = false;
    }

    private bool BeginFormFieldPlacement(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_placingFormField is null) return false;
        _formFieldPress = (pageIndex, ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position));
        slot.Annotations.CapturePointer(args.Pointer);
        args.Handled = true;
        return true;
    }

    private async Task<bool> EndFormFieldPlacementAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_placingFormField is not { } kind) return false;
        if (_formFieldPress is not { PageIndex: var pressPage, Origin: var origin } ||
            pressPage != pageIndex || _creatingFormField || _session is null) return true;
        _formFieldPress = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);

        var sessionId = _session.SessionId;
        var generation = _formPlacementGeneration;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        var page = _session.Pages[pageIndex];
        var width = kind == FieldToPlace.Checkbox ? 18 : 144;
        var height = kind switch
        {
            FieldToPlace.Checkbox => 18,
            FieldToPlace.RadioGroup => 48,
            _ => 36,
        };
        var rect = FormPlacementRect.Resolve(origin, point, page, width, height);
        if (rect.Width <= 0 || rect.Height <= 0) return true;

        _creatingFormField = true;
        try
        {
            var result = kind switch
            {
                FieldToPlace.Text => await _facade.AddTextFieldAsync(sessionId, (uint)pageIndex, rect),
                FieldToPlace.Checkbox => await _facade.AddCheckboxAsync(sessionId, (uint)pageIndex, rect),
                FieldToPlace.RadioGroup => await _facade.AddRadioGroupAsync(sessionId, (uint)pageIndex, rect),
                _ => await _facade.AddDropdownAsync(sessionId, (uint)pageIndex, rect),
            };
            if (_session?.SessionId != sessionId) return true;
            if (!result.IsSuccess)
            {
                FormFieldsStatus.Text = result.Error!.Message;
                return true;
            }

            _annotationState = result.Value!;
            UpdateAnnotationControls(_annotationState);
            _filledFieldPages.Add((uint)pageIndex);
            InvalidatePageRender((uint)pageIndex);
            await RefreshFormFieldsAsync();
            if (_session?.SessionId != sessionId) return true;
            if (generation == _formPlacementGeneration)
            {
                FormFieldsStatus.Text = kind switch
                {
                    FieldToPlace.Text => "Text field placed. Save to keep the change.",
                    FieldToPlace.Checkbox => "Checkbox placed. Save to keep the change.",
                    FieldToPlace.RadioGroup => "Radio group placed. Save to keep the change.",
                    _ => "Dropdown placed. Save to keep the change.",
                };
                StopPlacingFormField();
            }
        }
        finally
        {
            _creatingFormField = false;
        }
        return true;
    }
}
