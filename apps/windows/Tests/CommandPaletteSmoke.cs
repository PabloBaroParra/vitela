using System.Runtime.InteropServices;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Opt-in native-input harness for the command palette (Ctrl+K). It injects
/// real keyboard input, so run it on an interactive desktop without touching
/// the keyboard or mouse until it exits. Set VITELA_SMOKE_OUTPUT to an
/// existing directory for its log.
/// </summary>
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
            await _window.CommandPaletteSmokeAsync();
            File.WriteAllText(Path.Combine(output, "command-palette-smoke.log"),
                "PASS Home card lists Ctrl+K; Ctrl+K from Home, editor and Organize; whole catalog on an empty query; refused rows stay open with their reason; " +
                "accent-free Spanish search; arrows, Enter and Esc; focus returns on close; Ctrl+Z/Ctrl+Y edit the query, not the document; " +
                "tool and form entries arm their buttons; recents first; Undo runs through the history.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "command-palette-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    private const ushort PaletteVkControl = 0x11, PaletteVkReturn = 0x0D, PaletteVkEscape = 0x1B, PaletteVkUp = 0x26, PaletteVkDown = 0x28;

    internal async Task CommandPaletteSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        async Task Wait(Func<bool> predicate, string message)
        {
            for (var i = 0; i < 200 && !predicate(); i++) await Task.Delay(25);
            Check(predicate(), message);
        }
        string? FirstId() => _paletteShown.Count > 0 ? _paletteShown[0].Entry.Id : null;
        bool SearchFocused() => PaletteSearchBox.FocusState != FocusState.Unfocused;

        await Wait(() => Content.XamlRoot is not null && _shellInitialized, "Window not loaded");
        PaletteSetForeground();
        static IEnumerable<DependencyObject> Descendants(DependencyObject parent)
        {
            yield return parent;
            for (var i = 0; i < Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChildrenCount(parent); i++)
                foreach (var child in Descendants(Microsoft.UI.Xaml.Media.VisualTreeHelper.GetChild(parent, i))) yield return child;
        }
        var cardTexts = Descendants(HomeToolsColumn).OfType<Microsoft.UI.Xaml.Controls.TextBlock>().Select(t => t.Text).ToList();
        Check(cardTexts.Contains("Command palette") && cardTexts.Contains("Ctrl+K"), "Home's shortcut card lists Ctrl+K");

        // Home, no document.
        HomeSearchBox.Focus(FocusState.Programmatic);
        await Wait(() => HomeSearchBox.FocusState != FocusState.Unfocused, "Home search focus");
        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen && SearchFocused(), "Ctrl+K must open the palette from Home with its search box focused");
        Check(PaletteList.Items.Count == PaletteCatalog.Entries.Count && PaletteList.SelectedIndex == 0, "An empty query lists the whole catalog, first row selected");
        Check(_paletteShown.Single(r => r.Entry.Command == PaletteCommand.Save).Refusal == "Open a PDF before saving.", "Save is refused without a document");
        Check(_paletteShown.Single(r => r.Entry.Command == PaletteCommand.ModeAnnotate).Refusal is null, "Modes stay available without a document");

        await PaletteTypeAsync("deshacer");
        await Wait(() => FirstId() == "edit.undo", "The Spanish title finds Undo");
        await PaletteKeyAsync(PaletteVkReturn);
        Check(IsPaletteOpen && _paletteShown[0].Refusal == "Nothing to undo.", "A refused row stays open with its reason");
        await PaletteChordAsync('Z');
        await Wait(() => PaletteSearchBox.Text != "deshacer", "Ctrl+Z undoes the query's typing");
        Check(IsPaletteOpen, "Ctrl+Z must not close the palette");

        await PaletteKeyAsync(PaletteVkDown);
        await Wait(() => PaletteList.SelectedIndex == 1, "Down moves the selection");
        await PaletteKeyAsync(PaletteVkUp);
        await Wait(() => PaletteList.SelectedIndex == 0, "Up moves it back");
        await PaletteKeyAsync(PaletteVkUp);
        Check(PaletteList.SelectedIndex == 0, "Up stops at the first row");

        await PaletteKeyAsync(PaletteVkEscape);
        await Wait(() => !IsPaletteOpen, "Esc closes the palette");
        await Wait(() => HomeSearchBox.FocusState != FocusState.Unfocused, "Focus returns to the Home search box");

        // A document with one undoable edit.
        await OpenDocumentAsync(SampleDisplayName, await File.ReadAllBytesAsync(SamplePath));
        await Wait(() => _session is not null && _annotationState?.SessionId == _session.SessionId, "Sample did not open");
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0, new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        var annotations = _annotationState!.Annotations.Count;
        Check(UndoButton.IsEnabled && annotations > 0, "The fixture edit is undoable");
        // The editor is up, so the Undo/Redo buttons' Ctrl+Z/Ctrl+Y accelerators are live.
        Check(EditorView.Visibility == Visibility.Visible && !_organizing, "The editor is showing");

        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen && SearchFocused(), "Ctrl+K must open the palette from the editor");
        await PaletteTypeAsync("abc");
        await Wait(() => PaletteSearchBox.Text == "abc", "Typing reaches the query");
        await PaletteChordAsync('Z');
        await Wait(() => PaletteSearchBox.Text != "abc", "Ctrl+Z in the query undoes the query");
        await PaletteChordAsync('Y');
        await Task.Delay(300);
        Check(_annotationState!.Annotations.Count == annotations && UndoButton.IsEnabled, "Ctrl+Z/Ctrl+Y in the query must not touch the document's history");

        await PaletteSelectAllAndTypeAsync("highlight");
        await Wait(() => FirstId() == "annotate.highlight", "highlight ranks first");
        await PaletteKeyAsync(PaletteVkReturn);
        await Wait(() => !IsPaletteOpen && HighlightButton.IsChecked == true && _armedAnnotation == AnnotationKind.Highlight, "Highlight entry arms its tool");
        Check(_selectedToolPage == "Annotate" && EditorView.Visibility == Visibility.Visible, "Highlight lands on the Annotate page");

        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen, "Palette reopens");
        Check(FirstId() == "annotate.highlight", "The command just run comes first");
        await PaletteTypeAsync("organize");
        await Wait(() => FirstId() == "mode.organize", "organize ranks first");
        await PaletteKeyAsync(PaletteVkReturn);
        await Wait(() => !IsPaletteOpen && _organizing, "Organize entry opens Organize");

        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen && SearchFocused(), "Ctrl+K must open the palette from Organize");
        await PaletteTypeAsync("campo de texto");
        await Wait(() => FirstId() == "forms.text", "campo de texto finds Text field");
        await PaletteKeyAsync(PaletteVkReturn);
        await Wait(() => !IsPaletteOpen && !_organizing && PlaceTextFieldButton.IsChecked == true, "Text field entry leaves Organize and arms placement");
        Check(_selectedToolPage == "Sign", "Text field lands on Fill & Sign");

        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen, "Palette reopens for Undo");
        await PaletteTypeAsync("undo");
        await Wait(() => FirstId() == "edit.undo" && _paletteShown[0].Refusal is null, "Undo is available with an edit");
        await PaletteKeyAsync(PaletteVkReturn);
        await Wait(() => _annotationState!.Annotations.Count == annotations - 1, "Undo entry undoes the document edit");

        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen, "Palette reopens for Ctrl+K close");
        await PaletteChordAsync('K');
        await Wait(() => !IsPaletteOpen, "Ctrl+K closes an open palette");
    }

    private async Task PaletteTypeAsync(string text)
    {
        foreach (var c in text)
        {
            PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { Scan = c, Flags = 0x0004 } } });
            PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { Scan = c, Flags = 0x0004 | 0x0002 } } });
        }
        await Task.Delay(200);
    }

    private async Task PaletteSelectAllAndTypeAsync(string text)
    {
        await PaletteChordAsync('A');
        await PaletteTypeAsync(text);
    }

    private async Task PaletteKeyAsync(ushort key)
    {
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = key } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = key, Flags = 0x0002 } } });
        await Task.Delay(200);
    }

    private async Task PaletteChordAsync(char letter)
    {
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = PaletteVkControl } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = letter } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = letter, Flags = 0x0002 } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = PaletteVkControl, Flags = 0x0002 } } });
        await Task.Delay(250);
    }

    private void PaletteSetForeground()
    {
        var handle = WinRT.Interop.WindowNative.GetWindowHandle(this);
        PaletteSetForegroundWindow(handle);
        if (PaletteGetForegroundWindow() != handle)
            throw new InvalidOperationException("The smoke window is not in the foreground; native keyboard input would go elsewhere.");
    }

    private static void PaletteSend(PaletteInput input)
    {
        if (PaletteSendInput(1, [input], Marshal.SizeOf<PaletteInput>()) != 1) throw new InvalidOperationException("Native input injection failed.");
    }

    [StructLayout(LayoutKind.Sequential)] private struct PaletteInput { public uint Type; public PaletteInputData Data; }
    [StructLayout(LayoutKind.Explicit)] private struct PaletteInputData
    {
        [FieldOffset(0)] public PaletteMouse Mouse;
        [FieldOffset(0)] public PaletteKey Key;
    }
    [StructLayout(LayoutKind.Sequential)] private struct PaletteMouse { public int X, Y; public uint Data, Flags, Time; public nuint Extra; }
    [StructLayout(LayoutKind.Sequential)] private struct PaletteKey { public ushort VirtualKey, Scan; public uint Flags, Time; public nuint Extra; }
    [DllImport("user32.dll", EntryPoint = "SendInput", SetLastError = true)] private static extern uint PaletteSendInput(uint count, PaletteInput[] inputs, int size);
    [DllImport("user32.dll", EntryPoint = "SetForegroundWindow")] private static extern bool PaletteSetForegroundWindow(nint window);
    [DllImport("user32.dll", EntryPoint = "GetForegroundWindow")] private static extern nint PaletteGetForegroundWindow();
}
