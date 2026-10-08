namespace Pdf.Windows;

public sealed partial class MainWindow
{
    /// <summary>Delete field against the real core: enable, remove, clear the selection, undo, reselect.</summary>
    internal async Task FormDeleteSmokeAsync(ulong fieldId)
    {
        static void check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        check(DeleteFormFieldButton.IsEnabled, "A selected field must enable Delete field");
        var fieldCount = _formFieldState!.Fields.Count;
        await DeleteSelectedFormFieldAsync();
        check(_formFieldState!.Fields.Count == fieldCount - 1 && _formFieldState.Fields.All(candidate => candidate.Id != fieldId), "Delete field must remove it from the real core");
        check(_selectedFormFieldId is null && !DeleteFormFieldButton.IsEnabled && !FormStyleFont.IsEnabled, "Delete field must clear the selection");
        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Count == fieldCount && _formFieldState.Fields.Any(candidate => candidate.Id == fieldId), "Undo must bring the deleted field back");
        SelectFormField(fieldId);
        check(SelectedFormField?.Style?.SizePt == 24 && DeleteFormFieldButton.IsEnabled, "The restored field keeps its style and can be selected again");
    }
}
