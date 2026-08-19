using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.ApplicationModel.DataTransfer;

namespace Pdf.Windows;

/// <summary>
/// Drag-select text and copy it to the clipboard. The geometry — turning a
/// point into a caret, a caret range into the rects to paint, and into the
/// text to copy — is <c>pdf_render::PageCharacters</c>, reached here through
/// the facade's <c>PageCharactersAsync</c> (see <c>core/pdf-ffi/src/selection.rs</c>).
/// The GTK shell asks the same questions of the same crate in-process; this
/// shell cannot link it directly, so it crosses the FFI instead of
/// reimplementing the hit-test math.
///
/// A drag-select is tried only after annotations refuse the gesture — an
/// armed tool, or a press that hits an existing annotation's handle or body,
/// is aimed at the annotation, not at the text underneath it. See
/// <c>MainWindow.Annotations.cs</c>'s <c>ConnectAnnotationPointer</c>.
/// </summary>
public sealed partial class MainWindow
{
    private readonly Dictionary<uint, PageTextState> _pageText = [];
    private TextDrag? _textSelection;
    /// <summary>
    /// Whether a text drag is currently in flight, distinct from
    /// <see cref="_textSelection"/> having a value: the selection itself
    /// survives past pointer-up so it stays copyable, but WinUI keeps
    /// delivering <c>PointerMoved</c> for ordinary hover once the button is
    /// no longer held. Without this flag, moving the mouse anywhere over the
    /// page after a release kept extending the old selection, because
    /// <c>ContinueTextSelection</c> had nothing but "is there a selection on
    /// this page" to go on.
    /// </summary>
    private bool _textDragActive;

    private void BeginTextSelection(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_session is null) return;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        _textSelection = new TextDrag(pageIndex, point, point);
        _textDragActive = true;
        slot.Annotations.CapturePointer(args.Pointer);
        RedrawSelection();
        _ = EnsurePageCharactersAsync((uint)pageIndex);
    }

    private void ContinueTextSelection(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (!_textDragActive) return;
        if (_textSelection is not { PageIndex: var dragPage } drag || dragPage != pageIndex) return;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        _textSelection = drag with { Focus = point };
        RedrawSelection();
    }

    private void EndTextSelection(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (!_textDragActive) return;
        _textDragActive = false;
        if (_textSelection is not { PageIndex: var dragPage } drag || dragPage != pageIndex) return;
        // WinUI does not guarantee a PointerMoved for the exact pixel the
        // pointer was at when the button came up — a fast drag can reach the
        // next line without one more move event firing before release. Without
        // this, the highlight (and the copy, which reads the same state) could
        // lag one line behind where the drag actually ended.
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        _textSelection = drag with { Focus = point };
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        RedrawSelection();
    }

    /// <summary>
    /// Copies the current selection's text, mirroring the Linux shell's
    /// <c>win.copy</c> accel. Stands down whenever an editable control holds
    /// focus, same guard <see cref="MainWindow.FileDrop"/>'s paste accelerator
    /// uses — otherwise this would swallow the copy every text field expects,
    /// starting with <see cref="SearchBox"/>.
    /// </summary>
    private void CopySelection_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (TextInputHasFocus())
        {
            args.Handled = false;
            return;
        }

        args.Handled = true;
        var text = SelectedText();
        if (text is not { Length: > 0 }) return;

        var package = new DataPackage();
        package.SetText(text);
        Clipboard.SetContent(package);
        AnnotationStatus.Text = "Text copied.";
    }

    private string? SelectedText()
    {
        if (_textSelection is not { } drag) return null;
        if (ResolveCaretRange(drag) is not (var characters, var anchor, var focus)) return null;
        return characters.TextIn(anchor, focus);
    }

    /// <summary>
    /// Loads and caches one page's characters, so every pointer-move of a
    /// drag resolves against the already-loaded handle instead of a facade
    /// round trip. A denial (the document forbids text extraction) is cached
    /// too, so a restricted document does not retry the facade on every
    /// press.
    /// </summary>
    private async Task EnsurePageCharactersAsync(uint pageIndex)
    {
        if (_session is null) return;
        if (!_pageText.TryGetValue(pageIndex, out var state))
        {
            state = new PageTextState();
            _pageText[pageIndex] = state;
        }

        if (state.Handle is not null || state.Denied || state.Loading) return;

        state.Loading = true;
        var sessionId = _session.SessionId;
        var result = await _facade.PageCharactersAsync(sessionId, pageIndex);
        state.Loading = false;

        if (_session is null || _session.SessionId != sessionId)
        {
            // A different document opened while this was in flight — the
            // cache it would populate has already been reset.
            result.Value?.Dispose();
            return;
        }

        if (!result.IsSuccess)
        {
            state.Denied = true;
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        state.Handle = result.Value;
        RedrawSelection();
    }

    private (PageCharacters Characters, uint Anchor, uint Focus)? ResolveCaretRange(TextDrag drag)
    {
        if (!_pageText.TryGetValue((uint)drag.PageIndex, out var state) || state.Handle is not { } characters) return null;
        if (characters.CaretAt(drag.Anchor.X, drag.Anchor.Y) is not { } anchor) return null;
        if (characters.CaretAt(drag.Focus.X, drag.Focus.Y) is not { } focus) return null;
        return (characters, anchor, focus);
    }

    private void RedrawSelection()
    {
        foreach (var slot in _slots) slot.Selection.Children.Clear();
        if (_textSelection is not { } drag) return;
        if (_session is null || drag.PageIndex >= _slots.Count || drag.PageIndex >= _session.Pages.Count) return;
        if (ResolveCaretRange(drag) is not (var characters, var anchor, var focus)) return;

        var page = _session.Pages[drag.PageIndex];
        var target = _slots[drag.PageIndex];
        var scale = target.Scale;
        foreach (var rect in characters.RectsIn(anchor, focus))
        {
            var rectangle = new Rectangle
            {
                Width = Math.Max(1, rect.WidthPt * scale),
                Height = Math.Max(1, rect.HeightPt * scale),
                Fill = new SolidColorBrush(global::Windows.UI.Color.FromArgb(90, 40, 120, 235)),
            };
            Canvas.SetLeft(rectangle, rect.XPt * scale);
            Canvas.SetTop(rectangle, (page.HeightPt - rect.YPt - rect.HeightPt) * scale);
            target.Selection.Children.Add(rectangle);
        }
    }

    /// <summary>Drops every cached page's characters and the live selection — called when a new document replaces the session.</summary>
    private void ResetSelectionState()
    {
        foreach (var state in _pageText.Values) state.Handle?.Dispose();
        _pageText.Clear();
        _textSelection = null;
        _textDragActive = false;
    }

    private sealed class PageTextState
    {
        public PageCharacters? Handle { get; set; }
        public bool Loading { get; set; }
        public bool Denied { get; set; }
    }

    /// <summary>A drag-select in progress or at rest, in PDF-space points on the page it started on.</summary>
    private readonly record struct TextDrag(int PageIndex, AnnotationPoint Anchor, AnnotationPoint Focus);
}
