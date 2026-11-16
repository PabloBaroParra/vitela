using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
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
/// </summary>
public sealed partial class MainWindow
{
    private readonly PaletteRecents _paletteRecents = new();
    /// <summary>The rows on screen, in order, with the refusal each was drawn with.</summary>
    private IReadOnlyList<PaletteResult> _paletteShown = [];
    /// <summary>The element that had the focus when the palette opened.</summary>
    private WeakReference<UIElement>? _palettePreviousFocus;

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
        if (IsPaletteOpen || _isBusy || _dialogOpen || _shellPickingFile || Content.XamlRoot is not { } root) return;
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
    /// can scroll the page — the jump the GTK content editor once had. The
    /// page scroller's offsets are pinned across the move for the same reason.
    /// </summary>
    private void ClosePalette()
    {
        if (!IsPaletteOpen) return;
        UIElement? previous = null;
        _palettePreviousFocus?.TryGetTarget(out previous);
        _palettePreviousFocus = null;
        var (left, top) = (PageScroller.HorizontalOffset, PageScroller.VerticalOffset);
        if (previous is not null && IsFocusableAfterPalette(previous)) previous.Focus(FocusState.Programmatic);
        PaletteLayer.Visibility = Visibility.Collapsed;
        if (PageScroller.HorizontalOffset != left || PageScroller.VerticalOffset != top)
            PageScroller.ChangeView(left, top, null, disableAnimation: true);
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
    /// Runs the row at <paramref name="index"/>. A refused row does nothing
    /// and the palette stays open with its reason on screen. Otherwise the
    /// palette closes first, focus handed back, so a flow that opens a dialog
    /// or a picker, or switches screens, starts from the window as it was;
    /// the entry counts as recent only once it actually ran.
    /// </summary>
    private async void ActivatePaletteRow(int index)
    {
        if (index < 0 || index >= _paletteShown.Count) return;
        var result = _paletteShown[index];
        if (result.Refusal is not null)
        {
            PaletteSearchBox.Focus(FocusState.Programmatic);
            return;
        }
        ClosePalette();
        if (await RunPaletteEntryAsync(result.Entry)) _paletteRecents.Record(result.Entry.Id);
    }
}
