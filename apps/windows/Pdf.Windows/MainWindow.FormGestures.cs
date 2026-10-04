using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Canvas gestures for form geometry; field values and commands stay in Facade.</summary>
public sealed partial class MainWindow
{
    private sealed record FormGeometryDrag(string SessionId, FormField Field, AnnotationPoint Origin,
        AnnotationPoint Current, Corner? Corner);
    private FormGeometryDrag? _formGeometryDrag;

    private bool BeginFormGeometryDrag(int pageIndex, AnnotationPoint point, double scale)
    {
        if (!_formEditMode || _placingFormField is not null || !EditFormsButton.IsEnabled
            || _dialogOpen || _session is null) return false;
        var selected = SelectedFormField is { Rect: { } rect } field && field.PageIndex == pageIndex ? field : null;
        Corner? corner = selected?.Rect is { } bounds ? CornerAt(bounds, point, HandleReachPx / scale) : null;
        var hit = selected is not null && (corner is not null || Contains(selected.Rect!, point))
            ? selected : _formFieldState?.Fields.LastOrDefault(f => f.PageIndex == pageIndex && f.Rect is { } box && Contains(box, point));
        if (hit is null)
        {
            SelectFormField(null);
            return false;
        }
        SelectFormField(hit.Id);
        // Moving focus can submit a pending fill/name edit. The facade rechecks
        // the captured geometry and permission before committing the gesture.
        if (_formFocusTargets.TryGetValue(hit.Id, out var target)) target.Focus(FocusState.Programmatic);
        _formGeometryDrag = new(_session.SessionId, hit, point, point, ReferenceEquals(hit, selected) ? corner : null);
        RedrawAnnotations();
        return true;
    }

    private void ContinueFormGesture(int pageIndex, AnnotationPoint point)
    {
        if (_formGeometryDrag is { Field.PageIndex: var dragPage } drag && dragPage == pageIndex)
            _formGeometryDrag = drag with { Current = point };
        if (_formFieldPress is { PageIndex: var pressPage, Origin: var origin } && pressPage == pageIndex)
            _formFieldPress = (pageIndex, origin, point);
        RedrawAnnotations();
    }

    private static AnnotationRect FormDragRect(FormGeometryDrag drag) => drag.Corner is { } corner
        ? ResizedRect(drag.Field.Rect!, corner, drag.Current)
        : MovedRect(drag.Field.Rect!, drag.Origin, drag.Current);

    private async Task CommitFormGeometryDragAsync(FormGeometryDrag drag)
    {
        if (_session?.SessionId != drag.SessionId || !_formEditMode || !EditFormsButton.IsEnabled
            || _dialogOpen || _selectedFormFieldId != drag.Field.Id || drag.Field.Rect is not { } before) return;
        var after = FormDragRect(drag);
        if (before == after) return;
        SetBusy(true);
        try
        {
            await _formFillTail;
            if (_session?.SessionId != drag.SessionId || !_formEditMode || _selectedFormFieldId != drag.Field.Id) return;
            var result = drag.Corner is null
                ? await _facade.MoveFormFieldAsync(drag.SessionId, drag.Field.Id, before, after.X, after.Y)
                : await _facade.ResizeFormFieldAsync(drag.SessionId, drag.Field.Id, before, after);
            await ShowFieldStyleResultAsync(drag.SessionId, drag.Field, result,
                drag.Corner is null ? "Field moved. Changes are pending save." : "Field resized. Changes are pending save.");
        }
        finally { SetBusy(false); RedrawAnnotations(); }
    }

    private void CancelFormGesture(int pageIndex)
    {
        if (_formGeometryDrag?.Field.PageIndex == pageIndex) _formGeometryDrag = null;
        if (_formFieldPress?.PageIndex == pageIndex) _formFieldPress = null;
        RedrawAnnotations();
    }
}
