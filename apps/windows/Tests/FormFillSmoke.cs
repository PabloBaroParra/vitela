using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

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
            await _window.FormFillSmokeAsync();
            File.WriteAllText(Path.Combine(output, "form-fill-smoke.log"),
                "PASS immediate ordered text writes; focus/row preservation; rename F2/Enter/Escape; checkbox/radio/dropdown; editable choice; unsupported/empty/permission/busy/stale-row gates; undo/redo.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "form-fill-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    [DllImport("user32.dll")]
    private static extern bool PostMessage(nint window, uint message, nuint key, nint data);
    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(nint window, out uint process);
    [DllImport("user32.dll")]
    private static extern bool GetGUIThreadInfo(uint thread, ref FillGuiThreadInfo info);
    [StructLayout(LayoutKind.Sequential)]
    private struct FillGuiThreadInfo
    {
        public uint Size, Flags;
        public nint Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public int Left, Top, Right, Bottom;
    }

    internal async Task FormFillSmokeAsync()
    {
        static void check(bool condition, string message)
        {
            if (!condition) throw new InvalidOperationException(message);
        }
        static async Task wait(Func<bool> ready, string message)
        {
            for (var attempt = 0; !ready() && attempt < 100; attempt++) await Task.Delay(30);
            check(ready(), message);
        }
        async Task key(byte code)
        {
            var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
            var thread = GetWindowThreadProcessId(handle, out var process);
            check(process == Environment.ProcessId, "Test handle belongs to another process");
            var info = new FillGuiThreadInfo { Size = (uint)Marshal.SizeOf<FillGuiThreadInfo>() };
            check(GetGUIThreadInfo(thread, ref info) && info.Focus != 0, "No native focus target");
            check(PostMessage(info.Focus, 0x100, code, 1), "Cannot post key down");
            check(PostMessage(info.Focus, 0x101, code, unchecked((nint)0xC0000001)), "Cannot post key up");
            await Task.Delay(100);
        }
        await wait(() => Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Fill smoke", await File.ReadAllBytesAsync(SamplePath));
        check(_session is not null, "Sample did not open");
        await RefreshAnnotationStateAsync();
        await RefreshFormFieldsAsync();
        SelectToolPage("Sign");
        var sessionId = _session!.SessionId;
        var textResult = await _facade.AddTextFieldAsync(sessionId, 0, new(40, 400, 144, 36));
        check(textResult.IsSuccess, "Text placement failed");
        await RefreshFormFieldsAsync();
        var field = _formFieldState!.Fields.Last();
        var id = field.Id;
        var box = (TextBox)_formFocusTargets[id];
        await wait(() => box.Focus(FocusState.Programmatic), "Cannot focus text field");
        box.Text = "A";
        box.Text = "Ad";
        box.Text = "Ada";
        await _formFillTail;
        var snapshot = await _facade.FormFieldsAsync(sessionId);
        check(snapshot.Value!.Fields.Single(candidate => candidate.Id == id).Value == new FormFieldValue.Text("Ada"), "Typing did not commit immediately/in order");
        check(ReferenceEquals(_formFocusTargets[id], box) && box.FocusState != FocusState.Unfocused, "Typing replaced row/focus");
        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Single(candidate => candidate.Id == id).Value == new FormFieldValue.Text("Ad"), "Undo did not preserve individual text changes");
        await ApplyHistoryAndMetadataAsync(false);
        check(_formFieldState!.Fields.Single(candidate => candidate.Id == id).Value == new FormFieldValue.Text("Ada"), "Redo failed");

        var row = (StackPanel)FormFieldRows.Children.Last();
        var name = (TextBox)row.Children[0];
        check(name.IsReadOnly, "Name not initially read-only");
        await wait(() => name.Focus(FocusState.Programmatic), "Name not focusable");
        await key(0x71); // F2
        await wait(() => !name.IsReadOnly, "F2 did not enter name editing");
        name.Text = "Cancelled name";
        await key(0x1B); // Escape
        check(name.IsReadOnly && name.Text == field.Name, "Escape did not cancel rename");
        await key(0x71);
        await wait(() => !name.IsReadOnly, "Second F2 did not enter name editing");
        name.Text = "Customer name";
        await key(0x0D); // Enter
        await wait(() => _formFieldState!.Fields.Any(candidate => candidate.Id == id && candidate.Name == "Customer name"), "Enter did not commit rename");
        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Single(candidate => candidate.Id == id).Name == field.Name, "Rename undo failed");
        await ApplyHistoryAndMetadataAsync(false);

        row = (StackPanel)FormFieldRows.Children.Last();
        name = (TextBox)row.Children[0];
        await wait(() => name.Focus(FocusState.Programmatic), "Cannot focus renamed field");
        await key(0x71);
        await wait(() => !name.IsReadOnly, "F2 did not reopen name editing");
        name.Text = "Focus loss name";
        check(EditFormsButton.Focus(FocusState.Programmatic), "Cannot leave name editor");
        await wait(() => _formFieldState!.Fields.Any(candidate => candidate.Id == id && candidate.Name == "Focus loss name"), "Focus loss did not commit rename");
        await ApplyHistoryAndMetadataAsync(true);
        check(_formFieldState!.Fields.Single(candidate => candidate.Id == id).Name == "Customer name", "Focus-loss rename undo failed");

        check((await _facade.AddCheckboxAsync(sessionId, 0, new(40, 350, 18, 18))).IsSuccess, "Checkbox placement failed");
        await RefreshFormFieldsAsync();
        var checkboxField = _formFieldState!.Fields.Last();
        var checkbox = (CheckBox)_formFocusTargets[checkboxField.Id];
        await wait(() => checkbox.Focus(FocusState.Programmatic), "Cannot focus checkbox");
        await key(0x20);
        await _formFillTail;
        check(_formFieldState!.Fields.Last().Value == new FormFieldValue.Checked(true), "Checkbox click did not commit");

        check((await _facade.AddRadioGroupAsync(sessionId, 0, new(40, 300, 72, 18))).IsSuccess, "Radio placement failed");
        await RefreshFormFieldsAsync();
        var radioField = _formFieldState!.Fields.Last();
        var radioRow = (StackPanel)FormFieldRows.Children.Last();
        var radioGroup = (StackPanel)radioRow.Children[1];
        var lastRadio = radioGroup.Children.OfType<RadioButton>().Last();
        lastRadio.IsChecked = true;
        await _formFillTail;
        check(_formFieldState!.Fields.Last().Value == new FormFieldValue.Choice(((FormFieldKind.RadioGroup)radioField.Kind).Options.Last()), "Radio selection did not commit export value");

        check((await _facade.AddDropdownAsync(sessionId, 0, new(40, 250, 120, 24))).IsSuccess, "Dropdown placement failed");
        await RefreshFormFieldsAsync();
        var dropdownField = _formFieldState!.Fields.Last();
        var dropdown = (ComboBox)_formFocusTargets[dropdownField.Id];
        check((string)dropdown.Items[0] == "(none)", "Missing no-choice item");
        dropdown.SelectedIndex = 1;
        await _formFillTail;
        check(_formFieldState!.Fields.Last().Value == new FormFieldValue.Choice(((FormFieldKind.Dropdown)dropdownField.Kind).Options[0]), "Dropdown selection did not commit");
        dropdown.SelectedIndex = 0;
        await _formFillTail;
        check(_formFieldState!.Fields.Last().Value == new FormFieldValue.Choice(null), "Dropdown none did not clear value");

        // Presentation-only states not emitted by built-in placement commands.
        var state = _formFieldState!;
        ShowFormFields(state with { StructureAllowed = false });
        var fillOnlyBox = (TextBox)_formFocusTargets[id];
        check(fillOnlyBox.IsEnabled, "Filling incorrectly requires structure permission");
        fillOnlyBox.Text = "Fill only";
        await _formFillTail;
        snapshot = await _facade.FormFieldsAsync(sessionId);
        check(snapshot.Value!.Fields.Single(candidate => candidate.Id == id).Value == new FormFieldValue.Text("Fill only"), "Fill-only UI did not submit value");
        fillOnlyBox.Text = "Ada";
        await _formFillTail;
        var choices = ((FormFieldKind.Dropdown)dropdownField.Kind).Options;
        var editable = dropdownField with { Kind = new FormFieldKind.Dropdown(choices, true), Value = new FormFieldValue.Choice(null) };
        ShowFormFields(state with { Fields = [editable] });
        var choiceBox = (TextBox)_formFocusTargets[editable.Id];
        choiceBox.Text = choices[0];
        await _formFillTail;
        snapshot = await _facade.FormFieldsAsync(sessionId);
        check(snapshot.Value!.Fields.Single(candidate => candidate.Id == editable.Id).Value == new FormFieldValue.Choice(choices[0]), "Editable choice typing did not commit");
        choiceBox.Text = "";
        await _formFillTail;
        snapshot = await _facade.FormFieldsAsync(sessionId);
        check(snapshot.Value!.Fields.Single(candidate => candidate.Id == editable.Id).Value == new FormFieldValue.Choice(null), "Editable choice empty typing did not clear value");
        var unsupported = field with { Id = ulong.MaxValue, Kind = new FormFieldKind.Unsupported() };
        ShowFormFields(state with { Fields = [editable, unsupported], FillAllowed = false, StructureAllowed = false });
        check(_formFocusTargets[editable.Id] is TextBox { IsEnabled: false }, "Editable dropdown permission gate");
        var unsupportedRow = (StackPanel)FormFieldRows.Children.Last();
        check(unsupportedRow.Children.Last() is TextBlock { Text: "Unsupported field" }, "Unsupported kind not explicit");
        check(FormFieldsStatus.Text.Contains("does not permit"), "Missing permission notice");
        ShowFormFields(state with { Fields = [] });
        check(FormFieldsStatus.Text == "Open a PDF with form fields to fill them in, or place one in Edit forms mode.", "Empty placeholder mismatch");
        var oldGeneration = _formRowsGeneration;
        ShowFormFields(state);
        await QueueFormFillAsync(sessionId, field, oldGeneration, new FormFieldValue.Text("stale"));
        SetBusy(true);
        check(!FormFieldsScroller.IsEnabled, "Busy fill gate");
        await QueueFormFillAsync(sessionId, field, _formRowsGeneration, new FormFieldValue.Text("busy"));
        SetBusy(false);
        snapshot = await _facade.FormFieldsAsync(sessionId);
        check(snapshot.Value!.Fields.Single(candidate => candidate.Id == id).Value == new FormFieldValue.Text("Ada"), "Stale/busy command mutated field");
        check(EditableChoice("") == new FormFieldValue.Choice(null) && EditableChoice("free text") == new FormFieldValue.Choice("free text"), "Editable choice conversion failed");
    }
}
