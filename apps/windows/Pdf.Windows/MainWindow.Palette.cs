using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// The command palette overlay (Ctrl+K), a port of the GTK shell's
/// <c>palette/mod.rs</c> (B27 T-230): opening over the window, the keys,
/// running an entry and handing the focus back on close. The catalog and its
/// gates are <see cref="PaletteCatalog"/>; what an entry runs is
/// <c>MainWindow.PaletteCommands.cs</c>; the rows are <c>MainWindow.PaletteRows.cs</c>.
///
/// The search box keeps its own Ctrl+C/V/X/Z/Y (GTK #581) with no extra code:
/// the window's clipboard accelerators already decline while a text box has
/// the focus (<c>TextInputHasFocus</c>), and a focused TextBox's built-in
/// Ctrl+Z/Ctrl+Y win over the Undo/Redo buttons' accelerators — measured by
/// <c>Tests/CommandPaletteSmoke</c>. Marking accelerator processing handled
/// there instead, as a first attempt did, also cancelled the box's own undo.
///
/// Otherwise the palette is modal, like the GTK dialog it ports: the
/// window's shortcuts — Ctrl+letters, Ctrl+Home/End/arrows, Delete, F2 —
/// and a file dropped on the scrim do nothing while it is open
/// (<see cref="PaletteLayer_ProcessKeyboardAccelerators"/>, <c>Window_Drop</c>,
/// and an <see cref="IsPaletteOpen"/> check in each key handler behind it),
/// so no flow starts under rows drawn for another state. Tab cycles inside
/// the card (<c>TabFocusNavigation="Cycle"</c>).
/// </summary>
public sealed partial class MainWindow
{
    private readonly PaletteRecents _paletteRecents = new();
    /// <summary>The rows on screen, in order, with the refusal each was drawn with.</summary>
    private IReadOnlyList<PaletteResult> _paletteShown = [];
    /// <summary>The element that had the focus when the palette opened.</summary>
    private WeakReference<UIElement>? _palettePreviousFocus;
    /// <summary>The session the rows on screen were drawn for.</summary>
    private string? _paletteSessionId;
    /// <summary>A palette-launched command is still awaiting; Ctrl+K stays shut until it ends.</summary>
    private bool _paletteCommandRunning;

    private bool IsPaletteOpen => PaletteLayer.Visibility == Visibility.Visible;

    /// <summary>Ctrl+K, from Home, the editor or Organize: opens the palette, or closes it when it is open.</summary>
    private void CommandPalette_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        if (IsPaletteOpen) ClosePalette();
        else OpenPalette();
    }

    /// <summary>
    /// Shows the palette with an empty query and the focus in its search box,
    /// remembering who had the focus. Not while the shell is busy, a dialog
    /// is up or a file picker is open — the same guards the other shell
    /// shortcuts keep — so an entry never starts behind another flow.
    /// </summary>
    private void OpenPalette()
    {
        if (IsPaletteOpen || _paletteCommandRunning || _isBusy || _dialogOpen || _shellPickingFile || Content.XamlRoot is not { } root) return;
        _palettePreviousFocus = FocusManager.GetFocusedElement(root) is UIElement focused ? new(focused) : null;
        // Setting an already empty box raises no TextChanged; the list must still be current.
        PaletteSearchBox.Text = "";
        RefillPalette();
        PaletteLayer.Visibility = Visibility.Visible;
        PaletteLayer.UpdateLayout();
        PaletteSearchBox.Focus(FocusState.Programmatic);
    }

    /// <summary>
    /// Hides the palette and gives the focus back explicitly. The focus moves
    /// before the layer collapses: collapsing the focused search box would
    /// leave WinUI to pick the next focus itself, and bringing that into view
    /// can scroll the page — the jump the GTK content editor once had. When
    /// the remembered element is gone, the page (or the checked mode-bar tab)
    /// takes it instead. The page scroller's offsets are pinned across the
    /// move for the same reason, checked once the layout has run: a
    /// bring-into-view lands in the layout pass, not in <c>Focus</c>.
    /// The returned task ends once that restore is done; an entry awaits it
    /// before it runs, so the restore never undoes the command's own scroll
    /// (Find's jump, Edit's landing at the top).
    /// </summary>
    private Task ClosePaletteAsync()
    {
        if (!IsPaletteOpen) return Task.CompletedTask;
        UIElement? previous = null;
        _palettePreviousFocus?.TryGetTarget(out previous);
        _palettePreviousFocus = null;
        var (left, top) = (PageScroller.HorizontalOffset, PageScroller.VerticalOffset);
        if (previous is null || !IsFocusableAfterPalette(previous) || !previous.Focus(FocusState.Programmatic))
            FocusAfterPaletteFallback();
        PaletteLayer.Visibility = Visibility.Collapsed;
        var restored = new TaskCompletionSource();
        var queued = DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () =>
        {
            if (!IsPaletteOpen && (PageScroller.HorizontalOffset != left || PageScroller.VerticalOffset != top))
                PageScroller.ChangeView(left, top, null, disableAnimation: true);
            restored.SetResult();
        });
        if (!queued) restored.SetResult();
        return restored.Task;
    }

    /// <summary>The plain close — Esc, Ctrl+K, a click outside: nothing runs after it.</summary>
    private void ClosePalette() => _ = ClosePaletteAsync();

    /// <summary>The page when it is on screen, otherwise the mode-bar tab of the screen that is.</summary>
    private void FocusAfterPaletteFallback()
    {
        if (IsFocusableAfterPalette(PageScroller) && PageScroller.Focus(FocusState.Programmatic)) return;
        FocusCheckedModeButton();
    }

    /// <summary>
    /// Raised on the focused element's ancestors before any accelerator runs;
    /// the focus never leaves the card while it is open, so every window
    /// shortcut passes here first. Ctrl+K (which closes it) and Tab always go
    /// through. In the search box only the Ctrl+letter shortcuts are
    /// swallowed, bar the box's own Ctrl+A/C/V/X/Z/Y — leaving those unhandled
    /// is what lets the box keep them (see the class summary); its caret keys
    /// go through, and the window handlers they could reach (Delete, F2,
    /// Ctrl+Home/End page jumps) decline while the palette is open. Anywhere
    /// else in the card — the list, after Tab — every key is swallowed, so
    /// Delete or Ctrl+End never reaches the page. The arrows, Enter and Esc
    /// were already taken by <see cref="PaletteCard_PreviewKeyDown"/>.
    /// </summary>
    private void PaletteLayer_ProcessKeyboardAccelerators(UIElement sender, ProcessKeyboardAcceleratorEventArgs args)
    {
        if (!IsPaletteOpen || args.Key == VirtualKey.Tab) return;
        if (args.Modifiers.HasFlag(VirtualKeyModifiers.Control) && args.Key == VirtualKey.K) return;
        if (TextInputHasFocus())
        {
            // The box keeps its caret and word keys (Ctrl+Left, Ctrl+Backspace…).
            if (!args.Modifiers.HasFlag(VirtualKeyModifiers.Control) || args.Key is < VirtualKey.A or > VirtualKey.Z) return;
            if (args.Key is VirtualKey.A or VirtualKey.C or VirtualKey.V or VirtualKey.X or VirtualKey.Z or VirtualKey.Y) return;
        }
        args.Handled = true;
    }

    /// <summary>Whether <paramref name="element"/> is still on screen in this window and outside the palette.</summary>
    private bool IsFocusableAfterPalette(UIElement element)
    {
        if (element.XamlRoot != Content.XamlRoot) return false;
        for (DependencyObject? current = element; current is not null; current = VisualTreeHelper.GetParent(current))
        {
            if (ReferenceEquals(current, PaletteLayer)) return false;
            if (current is UIElement { Visibility: Visibility.Collapsed }) return false;
            if (ReferenceEquals(current, Content)) return true;
        }
        // Not under the window content: a closed popup's child, for instance.
        return false;
    }

    private void PaletteSearchBox_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (IsPaletteOpen) RefillPalette();
    }

    /// <summary>
    /// Rebuilds the list for the current query: ranked by the matcher, or,
    /// with an empty query, this session's commands first and then the
    /// catalog (decision 8). The first row is selected.
    /// </summary>
    private void RefillPalette()
    {
        var query = PaletteSearchBox.Text;
        _paletteSessionId = _session?.SessionId;
        var found = PaletteCatalog.Search(query, PaletteContextNow());
        _paletteShown = string.IsNullOrWhiteSpace(query) ? _paletteRecents.Order(found, result => result.Entry.Id) : found;
        PaletteList.Items.Clear();
        foreach (var result in _paletteShown) PaletteList.Items.Add(PaletteRow(result));
        var empty = _paletteShown.Count == 0;
        PaletteEmpty.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;
        PaletteList.Visibility = empty ? Visibility.Collapsed : Visibility.Visible;
        SelectPaletteRow(0);
    }

    private void SelectPaletteRow(int index)
    {
        if (index < 0 || index >= PaletteList.Items.Count) return;
        PaletteList.SelectedIndex = index;
        PaletteList.ScrollIntoView(PaletteList.Items[index]);
    }

    /// <summary>
    /// Tunnelling, on the card: the arrows, Enter and Esc are the palette's
    /// before the search box or the list sees them (and before the page's
    /// own Esc accelerator could clear a content selection). Refused rows are
    /// selectable like any other: selecting one is how its reason is read.
    /// </summary>
    private void PaletteCard_PreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        switch (e.Key)
        {
            case VirtualKey.Up:
                SelectPaletteRow(Math.Max(0, PaletteList.SelectedIndex - 1));
                break;
            case VirtualKey.Down:
                SelectPaletteRow(Math.Min(PaletteList.Items.Count - 1, PaletteList.SelectedIndex + 1));
                break;
            case VirtualKey.Enter:
                ActivatePaletteRow(PaletteList.SelectedIndex);
                break;
            case VirtualKey.Escape:
                ClosePalette();
                break;
            default:
                return;
        }
        e.Handled = true;
    }

    private void PaletteList_ItemClick(object sender, ItemClickEventArgs e) =>
        ActivatePaletteRow(PaletteList.Items.IndexOf(e.ClickedItem));

    private void PaletteScrim_PointerPressed(object sender, PointerRoutedEventArgs e)
    {
        e.Handled = true;
        ClosePalette();
    }

    /// <summary>
    /// Runs the row at <paramref name="index"/>. Its availability is asked
    /// again against the state now, not the one the rows were drawn with:
    /// when that answer changed in either direction, or another document is
    /// open, the rows are redrawn, the same entry stays selected and nothing
    /// runs, so what runs is only ever what is on screen. A refused row does
    /// nothing and the palette stays open with its reason on screen.
    /// Otherwise the palette closes first, focus handed back and the page's
    /// scroll restored, so a flow that opens a dialog or a picker, or switches
    /// screens, starts from the window as it was; the entry counts as recent
    /// only once it actually ran.
    /// </summary>
    private async void ActivatePaletteRow(int index)
    {
        if (_paletteCommandRunning || index < 0 || index >= _paletteShown.Count) return;
        var shown = _paletteShown[index];
        var refusal = shown.Entry.Refusal(PaletteContextNow());
        if (_session?.SessionId != _paletteSessionId || refusal != shown.Refusal)
        {
            RefillPalette();
            SelectPaletteRow(_paletteShown.ToList().FindIndex(result => result.Entry.Id == shown.Entry.Id));
            PaletteSearchBox.Focus(FocusState.Programmatic);
            return;
        }
        if (refusal is not null)
        {
            PaletteSearchBox.Focus(FocusState.Programmatic);
            return;
        }
        // Set before the close: Ctrl+K stays shut while the restore and the command run.
        _paletteCommandRunning = true;
        try
        {
            await ClosePaletteAsync();
            if (await RunPaletteEntryAsync(shown.Entry)) _paletteRecents.Record(shown.Entry.Id);
        }
        finally
        {
            _paletteCommandRunning = false;
        }
    }
}
