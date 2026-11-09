using System.Reflection;
using System.Runtime.InteropServices;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Viewer;
using Windows.Foundation;
using Windows.System;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Hand-tool panning: hold Space and drag with the left button, or drag with
/// the middle button, to move the document under the pointer. Mirrors the GTK
/// shell's <c>app/pan.rs</c>; the rules live in <see cref="HandTool"/>.
///
/// Input only. The pan writes <see cref="PageScroller"/>'s offsets and nothing
/// else, so rendering and the reading position follow the same path a
/// scrollbar drag takes.
/// </summary>
public sealed partial class MainWindow
{
    private readonly HandTool _hand = new();
    /// <summary>Where the claimed press went down, in <see cref="PageScroller"/> coordinates.</summary>
    private Point _panOrigin;
    /// <summary>The scroller's own cursor, kept while the hand's is shown; null when none is.</summary>
    private InputCursor? _savedScrollerCursor;
    private bool _handCursorShown;

    /// <summary>
    /// <c>UIElement.ProtectedCursor</c> is the only cursor hook WinUI 3 has and
    /// it is protected; <c>ScrollViewer</c> is sealed, so it cannot be set from
    /// a subclass. The CsWinRT projection keeps it a real property, so it is
    /// reached by reflection (the app is not trimmed).
    /// </summary>
    private static readonly PropertyInfo? ProtectedCursorProperty =
        typeof(UIElement).GetProperty("ProtectedCursor", BindingFlags.Instance | BindingFlags.NonPublic);

    private void InitializePan()
    {
        // WinUI 3 has no tunneling pointer events. Presses on a page reach the
        // page overlay first, which asks TryBeginPan before any tool (see
        // ConnectAnnotationPointer); everything else on the canvas — gaps
        // between pages, margins — bubbles here. handledEventsToo: the
        // ScrollViewer and the overlays mark presses handled.
        PageScroller.AddHandler(UIElement.PointerPressedEvent, new PointerEventHandler(PageScroller_PanPressed), true);
        PageScroller.AddHandler(UIElement.PointerMovedEvent, new PointerEventHandler(PageScroller_PanMoved), true);
        PageScroller.AddHandler(UIElement.PointerReleasedEvent, new PointerEventHandler(PageScroller_PanReleased), true);
        PageScroller.AddHandler(UIElement.PointerCaptureLostEvent, new PointerEventHandler(PageScroller_PanCaptureLost), true);
        PageScroller.AddHandler(UIElement.PointerCanceledEvent, new PointerEventHandler(PageScroller_PanCanceled), true);

        // Preview (tunneling) on the window root: the focused control would
        // otherwise see Space first, and a focused button would activate on
        // it. Text fields are left alone by HandTool.PressSpace itself.
        Content.AddHandler(UIElement.PreviewKeyDownEvent, new KeyEventHandler(Root_PanPreviewKeyDown), true);
        Content.AddHandler(UIElement.PreviewKeyUpEvent, new KeyEventHandler(Root_PanPreviewKeyUp), true);

        // A Space release delivered to another window never reaches us.
        Activated += (_, e) =>
        {
            if (e.WindowActivationState == WindowActivationState.Deactivated) ResetPan();
        };
    }

    /// <summary>Nothing to pan without pages on screen, and nothing behind a modal or Organize.</summary>
    private bool CanPan() =>
        _session is { PageCount: > 0 } && _slots.Count > 0
        && PageScroller.Visibility == Visibility.Visible
        && !_organizing && !_dialogOpen;

    private void Root_PanPreviewKeyDown(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Space) return;
        // TextInputHasFocus (MainWindow.FileDrop.cs) covers TextBox, PasswordBox,
        // RichEditBox and AutoSuggestBox; NumberBox and editable ComboBox focus an inner TextBox.
        if (_hand.PressSpace(TextInputHasFocus(), PointerOverDocument(), CanPan())) e.Handled = true;
        ShowPanCursor();
    }

    private void Root_PanPreviewKeyUp(object sender, KeyRoutedEventArgs e)
    {
        if (e.Key != VirtualKey.Space) return;
        // Swallowed too: a focused button activates on Space's release.
        if (_hand.ReleaseSpace()) e.Handled = true;
        ShowPanCursor();
    }

    /// <summary>
    /// Whether the mouse is over <see cref="PageScroller"/> right now, so Space
    /// arms the hand only there and still activates a focused toolbar button
    /// elsewhere. Asked of the system cursor rather than tracked from
    /// PointerEntered/Exited, which bubble up from every child and only fire
    /// once the pointer moves (not, say, when a document opened from the
    /// keyboard appears under a resting pointer). The XAML root's origin is
    /// the window's client origin.
    /// </summary>
    private bool PointerOverDocument()
    {
        if (Content.XamlRoot is not { } root || PageScroller.ActualWidth <= 0 || PageScroller.ActualHeight <= 0) return false;
        if (!PanGetCursorPos(out var cursor) || !PanScreenToClient(WindowNative.GetWindowHandle(this), ref cursor)) return false;
        var scale = root.RasterizationScale;
        var point = PageScroller.TransformToVisual(null).Inverse.TransformPoint(new Point(cursor.X / scale, cursor.Y / scale));
        return point.X >= 0 && point.Y >= 0 && point.X < PageScroller.ActualWidth && point.Y < PageScroller.ActualHeight;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct CursorPoint { public int X, Y; }

    [DllImport("user32.dll", EntryPoint = "GetCursorPos")]
    private static extern bool PanGetCursorPos(out CursorPoint point);

    [DllImport("user32.dll", EntryPoint = "ScreenToClient")]
    private static extern bool PanScreenToClient(nint window, ref CursorPoint point);

    private void PageScroller_PanPressed(object sender, PointerRoutedEventArgs e) => TryBeginPan(e);

    /// <summary>
    /// A button went down over the document. True claims the press: it is
    /// marked handled and captured by the scroller, so its moves and release
    /// never reach the page tools (annotation placement, selection, content
    /// edit, forms). False leaves it to them untouched.
    /// </summary>
    private bool TryBeginPan(PointerRoutedEventArgs e)
    {
        if (e.Pointer.PointerDeviceType != PointerDeviceType.Mouse) return false;
        if (_hand.Dragging)
        {
            // The same press, seen again on its way up from a page overlay.
            e.Handled = true;
            return true;
        }
        var point = e.GetCurrentPoint(PageScroller);
        var button = point.Properties.PointerUpdateKind switch
        {
            PointerUpdateKind.LeftButtonPressed => PanButton.Primary,
            PointerUpdateKind.MiddleButtonPressed => PanButton.Middle,
            _ => PanButton.Other,
        };
        if (!_hand.Begin(button, CanPan(), PageScroller.HorizontalOffset, PageScroller.VerticalOffset)) return false;
        _panOrigin = point.Position;
        e.Handled = true;
        PageScroller.CapturePointer(e.Pointer);
        ShowPanCursor();
        return true;
    }

    private void PageScroller_PanMoved(object sender, PointerRoutedEventArgs e)
    {
        if (!_hand.Dragging) return;
        e.Handled = true;
        var position = e.GetCurrentPoint(PageScroller).Position;
        if (_hand.Update(position.X - _panOrigin.X, position.Y - _panOrigin.Y,
                PageScroller.ScrollableWidth, PageScroller.ScrollableHeight) is { } offsets)
        {
            PageScroller.ChangeView(offsets.X, offsets.Y, null, disableAnimation: true);
        }
    }

    private void PageScroller_PanReleased(object sender, PointerRoutedEventArgs e)
    {
        if (!_hand.Dragging) return;
        e.Handled = true;
        // Cleared before the release: releasing raises PointerCaptureLost
        // synchronously, which would otherwise end the drag a second time.
        _hand.End();
        PageScroller.ReleasePointerCapture(e.Pointer);
        ShowPanCursor();
    }

    private void PageScroller_PanCaptureLost(object sender, PointerRoutedEventArgs e)
    {
        // A page overlay's own capture loss bubbles here too; only ours ends a drag.
        if (!_hand.Dragging) return;
        _hand.End();
        ShowPanCursor();
    }

    private void PageScroller_PanCanceled(object sender, PointerRoutedEventArgs e)
    {
        if (!_hand.Dragging) return;
        _hand.End();
        PageScroller.ReleasePointerCapture(e.Pointer);
        ShowPanCursor();
    }

    /// <summary>Drop everything: the window lost focus.</summary>
    private void ResetPan()
    {
        var dragging = _hand.Dragging;
        _hand.Reset();
        if (dragging) PageScroller.ReleasePointerCaptures();
        ShowPanCursor();
    }

    /// <summary>
    /// Shows the cursor the hand calls for, restoring the scroller's own once
    /// it has nothing to show. WinUI has no stock open/closed hand: the link
    /// hand stands for "grab", the four-way move arrows for "grabbing".
    /// </summary>
    private void ShowPanCursor()
    {
        if (ProtectedCursorProperty is null) return;
        var shape = _hand.Cursor switch
        {
            PanCursor.Grab => InputSystemCursorShape.Hand,
            PanCursor.Grabbing => InputSystemCursorShape.SizeAll,
            _ => (InputSystemCursorShape?)null,
        };
        if (shape is { } wanted)
        {
            if (!_handCursorShown)
            {
                _savedScrollerCursor = ProtectedCursorProperty.GetValue(PageScroller) as InputCursor;
                _handCursorShown = true;
            }
            ProtectedCursorProperty.SetValue(PageScroller, InputSystemCursor.Create(wanted));
        }
        else if (_handCursorShown)
        {
            ProtectedCursorProperty.SetValue(PageScroller, _savedScrollerCursor);
            _savedScrollerCursor = null;
            _handCursorShown = false;
        }
    }
}
