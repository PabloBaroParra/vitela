using Pdf.Windows.Facade;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    /// <summary>
    /// Canvas move and corner resize against the real core, driven through the
    /// same press/drag/release steps the pointer handlers use.
    /// </summary>
    internal async Task FormGeometrySmokeAsync(ulong fieldId)
    {
        static void check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        async Task drag(AnnotationPoint from, AnnotationPoint to)
        {
            var field = SelectedFormField!;
            check(BeginFormGeometryDrag((int)field.PageIndex, from, _slots[(int)field.PageIndex].Scale), "Press on the selected field must start a drag");
            ContinueFormGesture((int)field.PageIndex, to);
            var gesture = _formGeometryDrag!;
            _formGeometryDrag = null;
            await CommitFormGeometryDragAsync(gesture);
        }

        check(_formEditMode && _selectedFormFieldId == fieldId && SelectedFormField!.Rect is not null, "Geometry smoke needs the field selected in Edit forms");
        var start = SelectedFormField!.Rect!;

        await drag(new(start.X + start.Width / 2, start.Y + start.Height / 2), new(start.X + start.Width / 2 + 20, start.Y + start.Height / 2 + 20));
        var moved = start with { X = start.X + 20, Y = start.Y + 20 };
        check(SelectedFormField?.Rect == moved, $"Body drag must move without resizing: {SelectedFormField?.Rect} vs {moved}");

        // Grab the handle off-centre: the corner follows the pointer's delta.
        await drag(new(moved.X + moved.Width - 3, moved.Y + moved.Height - 3), new(moved.X + moved.Width + 17, moved.Y + moved.Height + 7));
        var resized = moved with { Width = moved.Width + 20, Height = moved.Height + 10 };
        check(SelectedFormField?.Rect == resized, $"Corner drag must resize from the opposite corner: {SelectedFormField?.Rect} vs {resized}");

        await drag(new(resized.X + 5, resized.Y + 5), new(resized.X + 5, resized.Y + 5));
        check(SelectedFormField?.Rect == resized && _formGeometryDrag is null && !_isBusy, "A click without movement must not edit");

        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Single(f => f.Id == fieldId).Rect == moved, "Undo must restore the pre-resize rect");
        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Single(f => f.Id == fieldId).Rect == start, "Undo must restore the pre-move rect");
        SelectFormField(fieldId);
    }
}
