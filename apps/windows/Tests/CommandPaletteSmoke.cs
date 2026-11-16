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
                "accent-free Spanish search; arrows, Enter and Esc; focus returns on close; Ctrl+Z/Ctrl+Y edit the query, not the document; Ctrl+O/Ctrl+F ignored while open; " +
                "tool and form entries arm their buttons; recents first; Undo runs through the history; a refused-then-allowed row redraws without running; " +
                "Tab+Delete and Ctrl+End in the list leave the page alone; the focus fallback lands on the checked mode tab; Edit from Organize keeps its own scroll.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "command-palette-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    private const ushort PaletteVkControl = 0x11, PaletteVkReturn = 0x0D, PaletteVkEscape = 0x1B, PaletteVkUp = 0x26, PaletteVkDown = 0x28,
        PaletteVkTab = 0x09, PaletteVkDelete = 0x2E, PaletteVkEnd = 0x23;

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
        await PaletteChordAsync('O');
        await Task.Delay(300);
        Check(IsPaletteOpen && !_shellPickingFile, "Ctrl+O is ignored while the palette is open");

        await PaletteKeyAsync(PaletteVkDown);
        await Wait(() => PaletteList.SelectedIndex == 1, "Down moves the selection");
        await PaletteKeyAsync(PaletteVkUp);
        await Wait(() => PaletteList.SelectedIndex == 0, "Up moves it back");
        await PaletteKeyAsync(PaletteVkUp);
        Check(PaletteList.SelectedIndex == 0, "Up stops at the first row");

        await PaletteKeyAsync(PaletteVkEscape);
        await Wait(() => !IsPaletteOpen, "Esc closes the palette");
        await Wait(() => HomeSearchBox.FocusState != FocusState.Unfocused, "Focus returns to the Home search box");
        // With no page on screen and the remembered element gone, the checked mode tab takes the focus.
        FocusAfterPaletteFallback();
        var homeTab = _modeButtons.Single(entry => entry.Mode == ShellMode.Home).Button;
        await Wait(() => homeTab.FocusState != FocusState.Unfocused, "The palette's focus fallback lands on the checked Home tab");
        // Put the focus back where the rest of the run expects it.
        HomeSearchBox.Focus(FocusState.Programmatic);
        await Wait(() => HomeSearchBox.FocusState != FocusState.Unfocused, "Focus goes back to the Home search box");

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
        await PaletteChordAsync('F');
        await Task.Delay(300);
        Check(IsPaletteOpen && !_documentFindFlyout.IsOpen, "Ctrl+F is ignored while the palette is open");

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
        await Wait(() => !_paletteCommandRunning && !UndoButton.IsEnabled, "Nothing is left to undo");

        // Refused when drawn, allowed when chosen: Enter redraws and keeps the row, running nothing.
        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen, "Palette reopens for the refused Undo");
        await PaletteTypeAsync("undo");
        await Wait(() => FirstId() == "edit.undo" && _paletteShown[0].Refusal == "Nothing to undo.", "Undo is drawn refused");
        var before = _annotationState!.Annotations.Select(a => a.Id).ToHashSet();
        await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Shape, 0, new PdfCoreRect(40, 80, 100, 40), new PdfCoreColor(255, 220, 0)));
        var withEdit = _annotationState!.Annotations.Count;
        var added = _annotationState.Annotations.Single(a => !before.Contains(a.Id)).Id;
        Check(UndoButton.IsEnabled, "The edit made under the palette is undoable");
        await PaletteKeyAsync(PaletteVkReturn);
        await Task.Delay(300);
        Check(IsPaletteOpen && _annotationState!.Annotations.Count == withEdit, "A row that became allowed must not run on that Enter");
        Check(PaletteList.SelectedIndex >= 0 && _paletteShown[PaletteList.SelectedIndex] is { Entry.Id: "edit.undo", Refusal: null }, "The redrawn Undo row stays selected, now allowed");
        await PaletteKeyAsync(PaletteVkEscape);
        await Wait(() => !IsPaletteOpen, "Esc closes the redrawn palette");

        // Modal beyond Ctrl+letter: Tab into the list, then Delete and Ctrl+End stay in the palette.
        // Delete's accelerator lives on the Annotate page's toolbar, live only while it is shown.
        ShowPaletteToolsPage("Annotate");
        await Task.Delay(300);
        while (_selectedAnnotationId != added) NavigateAnnotation(forward: true);
        var selected = _selectedAnnotationId;
        Check(selected is not null && DeleteAnnotationButton.IsEnabled, "An annotation is selected for Delete");
        // A probe on the window root sees every Delete / Ctrl+End accelerator the page would.
        var probed = 0;
        var reached = new HashSet<global::Windows.System.VirtualKey>();
        var probes = new List<Microsoft.UI.Xaml.Input.KeyboardAccelerator>();
        foreach (var (key, modifiers) in new[] { (global::Windows.System.VirtualKey.Delete, global::Windows.System.VirtualKeyModifiers.None), (global::Windows.System.VirtualKey.End, global::Windows.System.VirtualKeyModifiers.Control) })
        {
            var probe = new Microsoft.UI.Xaml.Input.KeyboardAccelerator { Key = key, Modifiers = modifiers };
            probe.Invoked += (_, args) => { probed++; reached.Add(args.KeyboardAccelerator.Key); };
            ((UIElement)Content).KeyboardAccelerators.Add(probe);
            probes.Add(probe);
        }
        PageScroller.Focus(FocusState.Programmatic);
        await PaletteChordAsync('K');
        await Wait(() => IsPaletteOpen && SearchFocused(), "Palette opens over the selection");
        await Task.Delay(600);
        var offset = PageScroller.VerticalOffset;
        bool FocusInCard()
        {
            for (var node = Microsoft.UI.Xaml.Input.FocusManager.GetFocusedElement(Content.XamlRoot) as DependencyObject; node is not null; node = Microsoft.UI.Xaml.Media.VisualTreeHelper.GetParent(node))
                if (ReferenceEquals(node, PaletteCard)) return true;
            return false;
        }
        await PaletteKeyAsync(PaletteVkTab);
        Check(FocusInCard() && !SearchFocused(), "Tab moves into the list");
        await PaletteKeyAsync(PaletteVkDelete);
        await PaletteChordAsync((char)PaletteVkEnd);
        await Task.Delay(300);
        Check(probed == 0, $"Delete and Ctrl+End in the list must reach no window accelerator ({probed} did)");
        Check(IsPaletteOpen && _annotationState!.Annotations.Count == withEdit && _selectedAnnotationId == selected, "Delete in the list must not remove the selected annotation");
        Check(PageScroller.VerticalOffset == offset, $"Ctrl+End in the list must not move the page ({offset} -> {PageScroller.VerticalOffset} of {PageScroller.ScrollableHeight})");
        await PaletteKeyAsync(PaletteVkTab);
        await PaletteKeyAsync(PaletteVkTab);
        Check(FocusInCard(), "Tab stays inside the palette");
        await PaletteKeyAsync(PaletteVkEscape);
        await Wait(() => !IsPaletteOpen, "Esc closes the palette from the list");
        // Control: with the palette shut the same keys do reach the window's accelerators.
        await PaletteKeyAsync(PaletteVkDelete);
        await PaletteChordAsync((char)PaletteVkEnd);
        // By key, not by count: from a focused button outside any scroller WinUI raises an
        // unhandled window accelerator twice, and Ctrl+End reaches it only outside a scroller.
        await Wait(() => reached.Count == 2, $"Delete and Ctrl+End reach the window once the palette is closed ({string.Join(", ", reached)})");
        foreach (var probe in probes) ((UIElement)Content).KeyboardAccelerators.Remove(probe);

        // A command's own scroll survives the close: Edit from Organize lands where the mode bar's Edit lands.
        Check(PageScroller.ScrollableHeight > 0, "The sample scrolls, so the landing check means something");
        {
            async Task<double> EditFromOrganizeAsync(bool palette)
            {
                PageScroller.ChangeView(null, PageScroller.ScrollableHeight, null, disableAnimation: true);
                await Wait(() => PageScroller.VerticalOffset > 0, "The page scrolls down");
                await EnterOrganizeViewAsync();
                await Wait(() => _organizing, "Organize opens");
                // As after a page move in the grid: leaving re-shows the document from its first page.
                _viewerStale = true;
                if (palette)
                {
                    await PaletteChordAsync('K');
                    await Wait(() => IsPaletteOpen, "Palette opens over Organize");
                    await PaletteTypeAsync("edit");
                    await Wait(() => FirstId() == "mode.edit", "edit ranks Edit first");
                    await PaletteKeyAsync(PaletteVkReturn);
                }
                else await OpenModeAsync(ShellMode.Edit, new RoutedEventArgs());
                await Wait(() => !_organizing && !_paletteCommandRunning, "Edit leaves Organize");
                await Task.Delay(600);
                return PageScroller.VerticalOffset;
            }
            var tabLanding = await EditFromOrganizeAsync(palette: false);
            var paletteLanding = await EditFromOrganizeAsync(palette: true);
            Check(tabLanding < PageScroller.ScrollableHeight / 2, $"The Edit tab lands near the top after a reorganize ({tabLanding})");
            Check(Math.Abs(tabLanding - paletteLanding) < 1, $"Edit from the palette lands where the Edit tab's does ({paletteLanding} vs {tabLanding})");
        }

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
        // Delete, End, Home and the arrows are extended keys; without the flag they read as the keypad's.
        var extended = key is >= 0x21 and <= 0x2E ? 0x0001u : 0u;
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = key, Flags = extended } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = key, Flags = extended | 0x0002 } } });
        await Task.Delay(200);
    }

    private async Task PaletteChordAsync(char letter)
    {
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = PaletteVkControl } } });
        var extended = letter is >= (char)0x21 and <= (char)0x2E ? 0x0001u : 0u;
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = letter, Flags = extended } } });
        PaletteSend(new PaletteInput { Type = 1, Data = new PaletteInputData { Key = new PaletteKey { VirtualKey = letter, Flags = extended | 0x0002 } } });
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
