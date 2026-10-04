using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

// Opt-in native test entry point; the normal application is rebuilt after this harness.
public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.FormToolbarSmokeAsync();
            File.WriteAllText(Path.Combine(output, "form-toolbar-smoke.log"),
                "PASS toolbar; mode/type exclusivity; common selection inspector; real-core font/size edits; undo/redo; busy/permission/no-selection gates; annotation/content cross-disarm.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "form-toolbar-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task FormToolbarSmokeAsync()
    {
        static void check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        check(Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Form toolbar smoke", await File.ReadAllBytesAsync(SamplePath));
        check(_session is not null, "Sample did not open");
        await RefreshAnnotationStateAsync();
        await RefreshFormFieldsAsync();
        SelectToolPage("Sign");
        check(EditFormsButton.IsEnabled && !FormStyleFont.IsEnabled && !FormStyleSize.IsEnabled && !FormStyleColor.IsEnabled, "Initial toolbar/selection gates");
        EditFormsButton.IsChecked = true;
        EditFormsButton_Click(EditFormsButton, new RoutedEventArgs());
        for (var attempt = 0; !_formEditMode && attempt < 100; attempt++) await Task.Delay(50);
        check(_formEditMode && _placingFormField is null, "Edit forms must arm without placement");
        EditFormsButton.IsChecked = false;
        EditFormsButton_Click(EditFormsButton, new RoutedEventArgs());
        check(!_formEditMode, "Edit forms toggle must disarm");
        foreach (var kind in Enum.GetValues<FieldToPlace>())
        {
            await SetFormFieldPlacementAsync(kind, true);
            check(_formEditMode && EditFormsButton.IsChecked == true && _placingFormField == kind, "Placement must imply Edit forms");
            check(new[] { PlaceTextFieldButton, PlaceCheckboxButton, PlaceRadioGroupButton, PlaceDropdownButton }.Count(button => button.IsChecked == true) == 1, "Only one type may be armed");
        }
        await SetFormFieldPlacementAsync(FieldToPlace.Dropdown, false);
        check(_formEditMode && _placingFormField is null, "Disarming a type must retain Edit forms");
        var sessionId = _session!.SessionId;
        var added = await _facade.AddTextFieldAsync(sessionId, 0, new(40, 400, 144, 36));
        check(added.IsSuccess, "Real field placement failed");
        await RefreshFormFieldsAsync();
        var field = _formFieldState!.Fields.Last();
        SelectFormField(field.Id);
        check(FormStyleFont.IsEnabled && FormStyleSize.IsEnabled && FormStyleColor.IsEnabled, "Selected style must enable inspector");
        check(FormStyleFont.Items.Count == 3 && FormStyleSize.Minimum == 4 && FormStyleSize.Maximum == 400, "Inspector bounds/fonts");
        FormStyleFont.SelectedIndex = (int)FormFont.TimesRoman;
        for (var attempt = 0; SelectedFormField?.Style?.Font != FormFont.TimesRoman && attempt < 100; attempt++) await Task.Delay(50);
        check(SelectedFormField?.Style?.Font == FormFont.TimesRoman, "Font inspector did not commit");
        FormStyleSize.Value = 24;
        for (var attempt = 0; SelectedFormField?.Style?.SizePt != 24 && attempt < 100; attempt++) await Task.Delay(50);
        check(SelectedFormField?.Style?.SizePt == 24, "Size inspector did not commit");
        await ApplyHistoryAndMetadataAsync(true);
        check(SelectedFormField?.Style?.SizePt == 12 && FormStyleSize.Value == 12, "Undo must refresh selected style");
        await ApplyHistoryAndMetadataAsync(false);
        check(SelectedFormField?.Style?.SizePt == 24 && FormStyleSize.Value == 24, "Redo must refresh selected style");
        SetBusy(true);
        check(!EditFormsButton.IsEnabled && !PlaceTextFieldButton.IsEnabled && !FormStyleColor.IsEnabled, "Busy gates");
        SetBusy(false);
        var state = _formFieldState!;
        _formFieldState = state with { StructureAllowed = false };
        UpdateFormToolbar();
        check(!EditFormsButton.IsEnabled && !FormStyleFont.IsEnabled && !PlaceDropdownButton.IsEnabled, "Structure permission gates");
        _formFieldState = state;
        UpdateFormToolbar();
        Arm(AnnotationKind.Highlight);
        check(!_formEditMode && EditFormsButton.IsChecked == false && _placingFormField is null, "Annotations must disarm Edit forms");
        var name = (TextBox)((StackPanel)FormFieldRows.Children.Last()).Children[0];
        check(name.Focus(FocusState.Programmatic), "Fill row could not receive focus");
        await Task.Delay(50);
        check(_selectedFormFieldId is null && !FormStyleFont.IsEnabled, "Ordinary filling must not select a field for structural editing");
        await SetFormFieldPlacementAsync(FieldToPlace.Text, true);
        ClearFormFieldPlacement();
        check(EditFormsButton.Focus(FocusState.Programmatic), "Mode button could not receive focus");
        await Task.Delay(50);
        check(name.Focus(FocusState.Programmatic), "Fill row could not regain focus");
        for (var attempt = 0; _selectedFormFieldId != field.Id && attempt < 100; attempt++) await Task.Delay(20);
        check(_selectedFormFieldId == field.Id && FormStyleFont.IsEnabled,
            $"Fill-row focus must select the field: mode={_formEditMode}, selected={_selectedFormFieldId}, expected={field.Id}, enabled={FormStyleFont.IsEnabled}");
        SetContentEditMode(true);
        check(!_formEditMode && _placingFormField is null && EditFormsButton.IsChecked == false, "Content editing must disarm Edit forms");
        check(!FormStyleFont.IsEnabled && !FormStyleSize.IsEnabled && !FormStyleColor.IsEnabled, "Disarm must clear selection");
    }
}
