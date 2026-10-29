using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Multi-item selection on the canvas (B25 T-208): a press on empty page
/// drags a marquee that gathers every text block, image and shape it
/// touches; SHIFT+click adds or removes one item; a drag inside the
/// selection moves all of it, and Delete removes all of it — one undo step
/// each.
/// </summary>
/// <remarks>
/// A "table" is what a marquee over it gathers: its border paths and the
/// text inside them. The single-item selections — a block, an image, a
/// shape — stay as they were; this is a separate state, and at most one of
/// the two is ever set.
/// </remarks>
public sealed partial class MainWindow
{
    private enum SelectionGestureKind { Marquee, Drag }

    private sealed record SelectionGesture(string SessionId, uint PageIndex, SelectionGestureKind Kind,
        AnnotationPoint Origin, AnnotationPoint Current, bool Dragging, ContentSelection? Base);

    private static readonly SolidColorBrush MarqueeFill = new(global::Windows.UI.Color.FromArgb(28, 26, 89, 217));

    private const string ContentSelectionHint =
        "drag inside the dashed box to move them, press Delete to remove them, SHIFT+click to add or remove one.";

    /// <summary>What a marquee or SHIFT+click gathered, or <c>null</c>.</summary>
    private ContentSelection? _contentSelection;

    private SelectionGesture? _selectionGesture;

    /// <summary>Whether SHIFT was down on the press content-edit mode claimed, read before any await.</summary>
    private bool _contentPressShift;

    /// <summary>
    /// The box a block is outlined with — its own estimate for a lone,
    /// just-edited run, the core's joined box otherwise — so a marquee
    /// gathers what the reader sees outlined.
    /// </summary>
    private AnnotationRect DrawnBounds(ContentTextBlock block) => block.IsMultiRun ? block.Bounds : BoundsOf(block.First);

    /// <summary>
    /// Claims the press for a selection gesture, or returns false so the
    /// single-item gestures take it: SHIFT+click toggles the item under the
    /// pointer, a press inside the selection drags it, and a press on empty
    /// page starts a marquee.
    /// </summary>
    private bool BeginContentSelectionGesture(PageSlot slot, int pageIndex, string sessionId, PageContent content,
        AnnotationPoint point, PointerRoutedEventArgs? args)
    {
        if (_contentInsertKind is not null) return false;
        var page = (uint)pageIndex;
        var reach = HandleReachPx / slot.Scale;

        // A corner handle of the selected image or shape overhangs its box;
        // a press there is a resize, never a marquee.
        if ((_selectedContentImage is { } image && image.PageIndex == page && CornerAt(image.Bounds, point, reach) is not null)
            || (_selectedContentPath is { } shape && shape.PageIndex == page && CornerAt(shape.Bounds, point, reach) is not null))
        {
            return false;
        }

        var hitImage = content.Images.LastOrDefault(candidate => Contains(candidate.Bounds, point));
        var hitBlock = ContentHitTest.TextRunAt(content.TextRuns, BoundsOf, point.X, point.Y) is { } run ? content.BlockOf(run) : null;
        var hitPath = hitImage is null && hitBlock is null ? PathAt(content, point, reach) : null;

        if (_contentPressShift)
        {
            var current = SelectionToExtend(page);
            if (hitImage is not null) SetContentSelection(current.Toggle(hitImage));
            else if (hitBlock is not null) SetContentSelection(current.Toggle(hitBlock));
            else if (hitPath is not null) SetContentSelection(current.Toggle(hitPath));
            else
            {
                // SHIFT on empty page: a marquee that adds to what is selected.
                SetContentSelection(current.IsEmpty ? null : current);
                BeginSelectionGesture(slot, sessionId, page, SelectionGestureKind.Marquee, point, args, current);
            }
            return true;
        }

        if (_contentSelection is { } selection && selection.PageIndex == page
            && selection.Bounds(DrawnBounds) is { } bounds && Contains(Padded(bounds, reach), point))
        {
            BeginSelectionGesture(slot, sessionId, page, SelectionGestureKind.Drag, point, args, null);
            return true;
        }

        ClearContentSelection();
        if (hitImage is not null || hitBlock is not null || hitPath is not null) return false;

        ClearTextBlockSelection();
        if (_selectedContentImage is not null)
        {
            ClearContentImageSelection();
            RedrawContentImageSelection();
        }
        if (_selectedContentPath is not null)
        {
            ClearContentPathSelection();
            RedrawContentPathSelection();
        }
        BeginSelectionGesture(slot, sessionId, page, SelectionGestureKind.Marquee, point, args, null);
        return true;
    }

    /// <summary>
    /// What SHIFT+click extends: the multi-selection on this page, or else the
    /// single item already selected there, so SHIFT after a plain click adds
    /// a second item instead of starting over.
    /// </summary>
    private ContentSelection SelectionToExtend(uint page)
    {
        if (_contentSelection is { } existing && existing.PageIndex == page) return existing;
        var start = ContentSelection.Empty(page);
        if (_selectedTextBlock is { } block && block.PageIndex == page) start = start.Toggle(block);
        if (_selectedContentImage is { } image && image.PageIndex == page) start = start.Toggle(image);
        if (_selectedContentPath is { } path && path.PageIndex == page) start = start.Toggle(path);
        return start;
    }

    private void BeginSelectionGesture(PageSlot slot, string sessionId, uint page, SelectionGestureKind kind,
        AnnotationPoint point, PointerRoutedEventArgs? args, ContentSelection? baseSelection)
    {
        // Released before the page content arrived: a click, finished now.
        if (!_contentPointerDown && args is not null)
        {
            if (kind == SelectionGestureKind.Marquee && baseSelection is null) _ = OpenContentEditorAsync(page, point);
            return;
        }
        _selectionGesture = new(sessionId, page, kind, point, point, Dragging: false, baseSelection);
        // No pointer in the smoke harness, which drives the gesture by points.
        if (args is not null) slot.Annotations.CapturePointer(args.Pointer);
    }

    private void ContinueContentSelectionGesture(PageSlot slot, int pageIndex, AnnotationPoint point)
    {
        if (_selectionGesture is not { } gesture || gesture.PageIndex != pageIndex) return;
        var travelled = Math.Max(Math.Abs(point.X - gesture.Origin.X), Math.Abs(point.Y - gesture.Origin.Y)) * slot.Scale;
        _selectionGesture = gesture with { Current = point, Dragging = gesture.Dragging || travelled > TextDragThresholdPx };
        if (_selectionGesture.Dragging) DrawContentOutlines(gesture.PageIndex);
    }

    private async Task EndContentSelectionGestureAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_selectionGesture is not { } gesture || gesture.PageIndex != pageIndex) return;
        var completed = gesture with { Current = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position) };
        // Cleared before the release: releasing raises PointerCaptureLost
        // synchronously, and its cancel would drop the gesture being finished.
        _selectionGesture = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        await FinishContentSelectionGestureAsync(completed);
    }

    /// <summary>Ends the press in progress at <paramref name="point"/> — the smoke harness's release.</summary>
    private async Task CompleteContentSelectionGestureAsync(AnnotationPoint point, bool dragging)
    {
        if (_selectionGesture is not { } gesture) return;
        _selectionGesture = null;
        await FinishContentSelectionGestureAsync(gesture with { Current = point, Dragging = dragging });
    }

    private void CancelContentSelectionGesture(int pageIndex)
    {
        if (_selectionGesture?.PageIndex != pageIndex) return;
        _selectionGesture = null;
        DrawContentOutlines((uint)pageIndex);
    }

    private async Task FinishContentSelectionGestureAsync(SelectionGesture completed)
    {
        if (completed.Kind == SelectionGestureKind.Marquee)
        {
            FinishMarquee(completed);
            return;
        }

        DrawContentOutlines(completed.PageIndex);
        var dx = completed.Current.X - completed.Origin.X;
        var dy = completed.Current.Y - completed.Origin.Y;
        if (!completed.Dragging || (dx == 0 && dy == 0) || _contentSelection is not { } selection
            || _session?.SessionId != completed.SessionId || !_contentEditMode || !ContentEditButton.IsEnabled || _dialogOpen)
        {
            return;
        }

        SetBusy(true);
        try
        {
            var result = await _facade.MoveContentSelectionAsync(completed.SessionId, selection, dx, dy);
            if (_session?.SessionId != completed.SessionId) return;
            await AfterContentSelectionEditAsync(selection, result, "Selection moved. Save to keep the change.", keep: true);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void FinishMarquee(SelectionGesture completed)
    {
        if (!completed.Dragging)
        {
            DrawContentOutlines(completed.PageIndex);
            // A plain click on empty page says what a click there does.
            if (completed.Base is null) _ = OpenContentEditorAsync(completed.PageIndex, completed.Origin);
            return;
        }
        if (!_pageContent.TryGetValue(completed.PageIndex, out var state) || state.Content is not { } content)
        {
            DrawContentOutlines(completed.PageIndex);
            return;
        }

        var gathered = ContentSelection.Gather(content, MarqueeRect(completed), DrawnBounds);
        var selection = completed.Base ?? ContentSelection.Empty(completed.PageIndex);
        foreach (var block in gathered.Blocks.Where(block => !selection.Contains(block))) selection = selection.Toggle(block);
        foreach (var image in gathered.Images.Where(image => !selection.Contains(image))) selection = selection.Toggle(image);
        foreach (var path in gathered.Paths.Where(path => !selection.Contains(path))) selection = selection.Toggle(path);
        SetContentSelection(selection.IsEmpty ? null : selection);
        if (selection.IsEmpty)
        {
            DrawContentOutlines(completed.PageIndex);
            AnnotationStatus.Text = "Nothing there to select.";
        }
    }

    private static AnnotationRect MarqueeRect(SelectionGesture gesture) => new(
        Math.Min(gesture.Origin.X, gesture.Current.X), Math.Min(gesture.Origin.Y, gesture.Current.Y),
        Math.Abs(gesture.Current.X - gesture.Origin.X), Math.Abs(gesture.Current.Y - gesture.Origin.Y));

    private static AnnotationRect Padded(AnnotationRect rect, double by) =>
        new(rect.X - by, rect.Y - by, rect.Width + 2 * by, rect.Height + 2 * by);

    /// <summary>
    /// Makes <paramref name="selection"/> the multi-selection and lets go of
    /// any single-item one; <c>null</c> clears it.
    /// </summary>
    private void SetContentSelection(ContentSelection? selection)
    {
        var previous = _contentSelection;
        _contentSelection = selection;
        if (selection is not null)
        {
            if (_selectedTextBlock is not null)
            {
                _selectedTextBlock = null;
                UpdateEditTextSelection();
            }
            if (_selectedContentImage is not null)
            {
                ClearContentImageSelection();
                RedrawContentImageSelection();
            }
            if (_selectedContentPath is not null)
            {
                ClearContentPathSelection();
            }
            ClaimPageFocus();
            AnnotationStatus.Text = $"{selection.Count} {(selection.Count == 1 ? "item" : "items")} selected — {ContentSelectionHint}";
        }
        if (previous is not null && previous.PageIndex != selection?.PageIndex) DrawContentOutlines(previous.PageIndex);
        if (selection is not null) DrawContentOutlines(selection.PageIndex);
    }

    private void ClearContentSelection()
    {
        if (_contentSelection is not { } selection) return;
        _contentSelection = null;
        DrawContentOutlines(selection.PageIndex);
    }

    /// <summary>Delete with a multi-selection: every member goes, one undo step.</summary>
    private async Task DeleteContentSelectionAsync()
    {
        if (_session is null || _isBusy || _contentSelection is not { } selection) return;
        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            var result = await _facade.RemoveContentSelectionAsync(sessionId, selection);
            if (_session?.SessionId != sessionId) return;
            await AfterContentSelectionEditAsync(selection, result, "Selection deleted. Save to keep the change.", keep: false);
        }
        finally
        {
            SetBusy(false);
        }
    }

    /// <summary>
    /// Re-reads the edited page, re-renders it, and — after a move — keeps the
    /// same members selected where they landed, so the next drag or Delete
    /// acts on what is painted.
    /// </summary>
    private async Task AfterContentSelectionEditAsync(ContentSelection selection, OperationResult<AnnotationState> result,
        string done, bool keep)
    {
        var pageIndex = selection.PageIndex;
        _contentSelection = null;
        if (!result.IsSuccess)
        {
            DrawContentOutlines(pageIndex);
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        _contentEditedPages.Add(pageIndex);
        InvalidatePageCharacters(pageIndex);
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        await RereadPageContentAsync(pageIndex);
        if (keep && _pageContent.TryGetValue(pageIndex, out var state) && state.Content is { } content
            && selection.Reread(content) is { IsEmpty: false } moved)
        {
            _contentSelection = moved;
        }
        DrawContentOutlines(pageIndex);
        AnnotationStatus.Text = done;
        InvalidatePageRender(pageIndex);
    }

    /// <summary>
    /// The marquee being dragged, then the selection: each member boxed, and
    /// the dashed box around all of them — drawn where it would land while it
    /// is dragged.
    /// </summary>
    private void DrawContentSelection(PageSlot slot, uint pageIndex)
    {
        if (_selectionGesture is { Kind: SelectionGestureKind.Marquee, Dragging: true } marquee && marquee.PageIndex == pageIndex)
        {
            var area = new Rectangle
            {
                Stroke = TextBlockDragBrush,
                StrokeThickness = 1,
                StrokeDashArray = [3, 2],
                Fill = MarqueeFill,
                IsHitTestVisible = false,
            };
            PlaceOverPage(area, slot, (int)pageIndex, MarqueeRect(marquee));
            slot.Content.Children.Add(area);
        }

        if (_contentSelection is not { } selection || selection.PageIndex != pageIndex) return;
        var drag = _selectionGesture is { Kind: SelectionGestureKind.Drag, Dragging: true } moving && moving.PageIndex == pageIndex
            ? moving : null;
        AnnotationRect Placed(AnnotationRect box) => drag is null ? box : MovedRect(box, drag.Origin, drag.Current);

        var members = selection.Blocks.Select(DrawnBounds)
            .Concat(selection.Images.Select(image => image.Bounds))
            .Concat(selection.Paths.Select(path => path.Bounds));
        foreach (var box in members)
        {
            var outline = new Rectangle { Stroke = TextBlockDragBrush, StrokeThickness = 2, IsHitTestVisible = false };
            PlaceOverPage(outline, slot, (int)pageIndex, Placed(box), minSide: 2);
            slot.Content.Children.Add(outline);
        }
        if (selection.Bounds(DrawnBounds) is { } bounds)
        {
            var frame = new Rectangle
            {
                Stroke = TextBlockDragBrush,
                StrokeThickness = 1,
                StrokeDashArray = [4, 3],
                IsHitTestVisible = false,
            };
            PlaceOverPage(frame, slot, (int)pageIndex, Placed(Padded(bounds, 3)));
            slot.Content.Children.Add(frame);
        }
    }
}
