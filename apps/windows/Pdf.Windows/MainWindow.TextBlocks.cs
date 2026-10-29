using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Text blocks on the canvas (B25 T-207): runs on one line that read as one
/// piece of text are outlined, selected, dragged, retyped and deleted as a
/// unit.
/// </summary>
/// <remarks>
/// Text works like a text box in a slide editor (chosen by the user
/// 2026-10-29): a click <em>selects</em> the block, Delete removes it, and a
/// double-click — or F2 — opens the inline editor over it. A press becomes a
/// drag of the whole block once the pointer travels past
/// <see cref="TextDragThresholdPx"/>, one undo step. The editor itself is
/// <c>MainWindow.ContentEditor.cs</c>'s, opened over the block's box with the
/// block's text, and its writes go to the block retype instead of a run's.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>How far a press on text travels, in screen pixels, before it is a drag and not a click.</summary>
    private const double TextDragThresholdPx = 4;

    private sealed record TextBlockDrag(string SessionId, ContentTextBlock Block, AnnotationPoint Origin,
        AnnotationPoint Current, bool Dragging);

    private static readonly SolidColorBrush TextBlockDragBrush = new(global::Windows.UI.Color.FromArgb(255, 26, 89, 217));

    private TextBlockDrag? _textBlockDrag;

    private const string TextBlockSelectedHint =
        "Text selected — drag to move it, press Delete to remove it, double-click or F2 to retype it.";

    /// <summary>The block a click selected, or <c>null</c>.</summary>
    private ContentTextBlock? _selectedTextBlock;

    /// <summary>
    /// Whether the press content-edit mode claimed is still down. Set on the
    /// press and cleared on the release, both synchronously, so a gesture that
    /// starts after an await knows whether the reader already let go.
    /// </summary>
    private bool _contentPointerDown;

    /// <summary>
    /// Starts a text press on the block under <paramref name="point"/>, or
    /// returns false when no text is there. A press already released by the
    /// time the page content arrived is a click, and opens the editor now.
    /// </summary>
    private async Task<bool> BeginTextBlockGestureAsync(PageSlot slot, int pageIndex, string sessionId,
        PageContent content, AnnotationPoint point, PointerRoutedEventArgs? args)
    {
        if (ContentHitTest.TextRunAt(content.TextRuns, BoundsOf, point.X, point.Y) is not { } run
            || content.BlockOf(run) is not { } block)
        {
            return false;
        }

        if (!_contentPointerDown)
        {
            SelectTextBlock(block);
            return true;
        }

        _textBlockDrag = new(sessionId, block, point, point, Dragging: false);
        // No pointer in the smoke harness, which drives the gesture by points.
        if (args is not null) slot.Annotations.CapturePointer(args.Pointer);
        return true;
    }

    private void ContinueTextBlockGesture(PageSlot slot, int pageIndex, AnnotationPoint point)
    {
        if (_textBlockDrag is not { } drag || drag.Block.PageIndex != pageIndex) return;
        var travelled = Math.Max(Math.Abs(point.X - drag.Origin.X), Math.Abs(point.Y - drag.Origin.Y)) * slot.Scale;
        _textBlockDrag = drag with { Current = point, Dragging = drag.Dragging || travelled > TextDragThresholdPx };
        if (_textBlockDrag.Dragging) DrawContentOutlines((uint)pageIndex);
    }

    private async Task EndTextBlockGestureAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_textBlockDrag is not { } drag || drag.Block.PageIndex != pageIndex) return;
        var completed = drag with { Current = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position) };
        // Cleared before the release, never after: releasing raises
        // PointerCaptureLost synchronously, and its cancel would otherwise
        // throw away the very gesture being finished.
        _textBlockDrag = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        await FinishTextBlockGestureAsync(completed);
    }

    /// <summary>Ends the press in progress at <paramref name="point"/> — the smoke harness's release.</summary>
    private async Task CompleteTextBlockGestureAsync(AnnotationPoint point, bool dragging)
    {
        if (_textBlockDrag is not { } drag) return;
        _textBlockDrag = null;
        await FinishTextBlockGestureAsync(drag with { Current = point, Dragging = dragging });
    }

    private async Task FinishTextBlockGestureAsync(TextBlockDrag completed)
    {
        if (!completed.Dragging)
        {
            SelectTextBlock(completed.Block);
            return;
        }

        DrawContentOutlines(completed.Block.PageIndex);
        var dx = completed.Current.X - completed.Origin.X;
        var dy = completed.Current.Y - completed.Origin.Y;
        if ((dx == 0 && dy == 0) || _session?.SessionId != completed.SessionId || !_contentEditMode
            || !ContentEditButton.IsEnabled || _dialogOpen)
        {
            return;
        }

        SetBusy(true);
        try
        {
            var result = await _facade.MoveTextBlockAsync(completed.SessionId, completed.Block, dx, dy);
            if (_session?.SessionId != completed.SessionId) return;
            await AfterTextBlockEditAsync(completed.Block.PageIndex, result, "Text moved. Save to keep the change.", completed.Block);
        }
        finally
        {
            SetBusy(false);
        }
    }

    private void CancelTextBlockGesture(int pageIndex)
    {
        _contentPointerDown = false;
        if (_textBlockDrag?.Block.PageIndex != pageIndex) return;
        var wasDragging = _textBlockDrag.Dragging;
        _textBlockDrag = null;
        if (wasDragging) DrawContentOutlines((uint)pageIndex);
    }

    /// <summary>
    /// Re-reads the edited page, so the outlines and the next click describe
    /// the blocks the core now reports — a retyped block is one run, a deleted
    /// one is gone — and re-renders it.
    /// </summary>
    private async Task AfterTextBlockEditAsync(uint pageIndex, OperationResult<AnnotationState> result, string done,
        ContentTextBlock? reselect = null)
    {
        _selectedTextBlock = null;
        if (!result.IsSuccess)
        {
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        _contentEditedPages.Add(pageIndex);
        InvalidatePageCharacters(pageIndex);
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        AnnotationStatus.Text = done;
        await RereadPageContentAsync(pageIndex);
        // A moved block stays selected where it landed, so the next drag or
        // Delete acts on what is painted.
        if (reselect is not null && _pageContent.TryGetValue(pageIndex, out var state) && state.Content is { } content)
        {
            _selectedTextBlock = content.TextBlocks.FirstOrDefault(block => block.Runs.Any(run => run.Id == reselect.First.Id));
            DrawContentOutlines(pageIndex);
        }
        UpdateEditTextSelection();
        InvalidatePageRender(pageIndex);
    }

    private void SelectTextBlock(ContentTextBlock block)
    {
        var previous = _selectedTextBlock;
        _selectedTextBlock = block;
        if (_selectedContentImage is not null)
        {
            ClearContentImageSelection();
            RedrawContentImageSelection();
        }
        ClearContentPathSelection();
        AnnotationStatus.Text = TextBlockSelectedHint;
        UpdateEditTextSelection();
        ClaimPageFocus();
        if (previous is not null && previous.PageIndex != block.PageIndex) DrawContentOutlines(previous.PageIndex);
        DrawContentOutlines(block.PageIndex);
    }

    /// <summary>
    /// Takes focus off a text field when something on the page is selected.
    /// A press on the page does not take focus, so it can still sit in the
    /// search box or an editor just closed, where Delete means "remove a
    /// character". Selecting something is a statement that the next key is
    /// for the page.
    /// </summary>
    private void ClaimPageFocus()
    {
        if (TextInputHasFocus()) PageScroller.Focus(FocusState.Programmatic);
    }

    private void ClearTextBlockSelection()
    {
        if (_selectedTextBlock is not { } selected) return;
        _selectedTextBlock = null;
        UpdateEditTextSelection();
        DrawContentOutlines(selected.PageIndex);
    }

    /// <summary>A double-click on text: the inline editor over the block under the pointer.</summary>
    private async Task OpenContentEditorOnDoubleTapAsync(uint pageIndex, AnnotationPoint point)
    {
        if (!_contentEditMode || _contentInsertKind is not null || _isBusy || _dialogOpen) return;
        ClearTextBlockSelection();
        ClearContentSelection();
        await OpenContentEditorAsync(pageIndex, point);
    }

    /// <summary>F2: the inline editor over the selected block.</summary>
    private async Task RetypeSelectedTextBlockAsync()
    {
        if (_selectedTextBlock is not { } block) return;
        var bounds = BoundsOf(block.First);
        ClearTextBlockSelection();
        await OpenContentEditorAsync(block.PageIndex, new AnnotationPoint(bounds.X + bounds.Width / 2, bounds.Y + bounds.Height / 2));
    }

    /// <summary>Delete with a block selected and no editor open: the whole block goes, one undo step.</summary>
    private async Task DeleteSelectedTextBlockAsync()
    {
        if (_session is null || _isBusy || _selectedTextBlock is not { } block) return;
        var sessionId = _session.SessionId;
        SetBusy(true);
        try
        {
            var result = await _facade.RemoveTextBlockAsync(sessionId, block);
            if (_session?.SessionId != sessionId) return;
            await AfterTextBlockEditAsync(block.PageIndex, result, "Text deleted. Save to keep the change.");
        }
        finally
        {
            SetBusy(false);
            UpdateEditTextSelection();
        }
    }

    /// <summary>
    /// Delete for whatever Edit content has selected — a text block, an image
    /// or a shape — and F2 to retype a selected block. Both stand down while a
    /// text field has focus: Delete there removes a character.
    /// </summary>
    /// <remarks>
    /// Hung on the page view, not on the Edit card's buttons: an accelerator
    /// only fires while its element is shown, and the card is collapsed
    /// whenever the tools panel is folded or on another tab — which is how
    /// Delete first did nothing on a selected block.
    /// </remarks>
    private void WireTextBlockKeys()
    {
        // One Delete for whatever Edit content has selected — text, an image
        // or a shape; at most one of them is selected at a time.
        var delete = new KeyboardAccelerator { Key = global::Windows.System.VirtualKey.Delete };
        delete.Invoked += async (_, args) =>
        {
            if (TextInputHasFocus() || !_contentEditMode || _pump.Box is not null) return;
            if (_contentSelection is not null)
            {
                args.Handled = true;
                await DeleteContentSelectionAsync();
            }
            else if (_selectedTextBlock is not null)
            {
                args.Handled = true;
                await DeleteSelectedTextBlockAsync();
            }
            else if (_selectedContentImage is not null && DeleteImageButton.IsEnabled)
            {
                args.Handled = true;
                await DeleteSelectedImageAsync();
            }
            else if (_selectedContentPath is not null && _deleteShapeButton.IsEnabled)
            {
                args.Handled = true;
                await DeleteSelectedPathAsync();
            }
        };
        PageScroller.KeyboardAccelerators.Add(delete);

        var retype = new KeyboardAccelerator { Key = global::Windows.System.VirtualKey.F2 };
        retype.Invoked += async (_, args) =>
        {
            if (TextInputHasFocus() || _selectedTextBlock is null) return;
            args.Handled = true;
            await RetypeSelectedTextBlockAsync();
        };
        PageScroller.KeyboardAccelerators.Add(retype);

        var deselect = new KeyboardAccelerator { Key = global::Windows.System.VirtualKey.Escape };
        deselect.Invoked += (_, args) =>
        {
            if (TextInputHasFocus() || _contentSelection is null) return;
            args.Handled = true;
            ClearContentSelection();
            AnnotationStatus.Text = "Selection cleared.";
        };
        PageScroller.KeyboardAccelerators.Add(deselect);
    }

    /// <summary>Drops one page's parsed content and its estimated boxes, then reads it again.</summary>
    private async Task RereadPageContentAsync(uint pageIndex)
    {
        _pageContent.Remove(pageIndex);
        foreach (var key in _pendingRunBounds.Keys.Where(key => key.Page == pageIndex).ToArray())
            _pendingRunBounds.Remove(key);
        if (pageIndex < _slots.Count) ClearContentOutlines(_slots[(int)pageIndex]);
        await EnsurePageContentAsync(pageIndex);
    }

    /// <summary>
    /// The run an editor opened over a whole block stands on: the first run's
    /// font and identity, with the block's box and text.
    /// </summary>
    private static ContentTextRun BlockStandIn(ContentTextBlock block)
    {
        var bounds = block.Bounds;
        return new ContentTextRun(
            block.First.Source with { Bbox = new PdfCoreRect(bounds.X, bounds.Y, bounds.Width, bounds.Height), Text = block.Text },
            block.First.BaseFont);
    }

    /// <summary>
    /// The outlines of one page's text: one box per block, solid when the
    /// font is preserved, dashed when editing substitutes one. A block being
    /// dragged is drawn where it would land.
    /// </summary>
    private void DrawTextBlockOutlines(PageSlot slot, uint pageIndex, PageContent content)
    {
        IEnumerable<(AnnotationRect Bounds, bool Substitutes, ContentTextBlock? Block)> boxes = content.TextBlocks.Count > 0
            ? content.TextBlocks.Select(block => (block.IsMultiRun ? block.Bounds : BoundsOf(block.First),
                block.First.RequiresFontSubstitution, (ContentTextBlock?)block))
            : content.TextRuns.Select(run => (BoundsOf(run), run.RequiresFontSubstitution, (ContentTextBlock?)null));
        foreach (var (bounds, substitutes, block) in boxes)
        {
            var dragged = _textBlockDrag is { Dragging: true } drag && block is not null && ReferenceEquals(drag.Block, block);
            var selected = !dragged && block is not null && ReferenceEquals(_selectedTextBlock, block);
            var outline = new Rectangle
            {
                StrokeThickness = dragged || selected ? 2 : 1,
                Stroke = dragged || selected ? TextBlockDragBrush : new SolidColorBrush(substitutes
                    ? global::Windows.UI.Color.FromArgb(120, 170, 90, 220)
                    : global::Windows.UI.Color.FromArgb(120, 40, 120, 235)),
                IsHitTestVisible = false,
            };
            if (substitutes && !dragged && !selected)
            {
                outline.StrokeDashArray = [2, 2];
            }

            var placed = dragged ? MovedRect(bounds, _textBlockDrag!.Origin, _textBlockDrag.Current) : bounds;
            PlaceOverPage(outline, slot, (int)pageIndex, placed);
            slot.Content.Children.Add(outline);
        }
    }
}
