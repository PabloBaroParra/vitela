using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>Canvas selection and geometry gestures for page-content images.</summary>
public sealed partial class MainWindow
{
    private sealed record ContentImageDrag(string SessionId, ContentImage Image, AnnotationPoint Origin,
        AnnotationPoint Current, Corner? Corner);

    private ContentImage? _selectedContentImage;
    private ContentImageDrag? _contentImageDrag;
    private readonly HashSet<(uint Page, ulong Image)> _pendingImageEdits = [];

    private async Task BeginContentGestureAsync(PageSlot slot, int pageIndex, AnnotationPoint point,
        Microsoft.UI.Xaml.Input.PointerRoutedEventArgs args)
    {
        if (_session is null || !_contentEditMode || !ContentEditButton.IsEnabled || _isBusy || _dialogOpen)
        {
            return;
        }

        var sessionId = _session.SessionId;
        await CommitContentEditorAsync();
        if (_session?.SessionId != sessionId || !_contentEditMode || !ContentEditButton.IsEnabled || _isBusy || _dialogOpen)
        {
            return;
        }

        var content = await EnsurePageContentAsync((uint)pageIndex);
        if (_session?.SessionId != sessionId || !_contentEditMode || content is null)
        {
            return;
        }

        var selected = _selectedContentImage is { PageIndex: var selectedPage } current && selectedPage == pageIndex
            ? current : null;
        var corner = selected is not null ? CornerAt(selected.Bounds, point, HandleReachPx / slot.Scale) : null;
        var hit = selected is not null && (corner is not null || Contains(selected.Bounds, point))
            ? selected
            : content.Images.LastOrDefault(image => Contains(image.Bounds, point));
        // A press anywhere but on text lets go of the selected text block.
        if (hit is not null || !content.TextRuns.Any(run => Contains(BoundsOf(run), point)))
        {
            ClearTextBlockSelection();
        }
        if (hit is null)
        {
            if (_selectedContentImage is not null)
            {
                _selectedContentImage = null;
                RedrawContentImageSelection();
            }
            // Text outranks a path: a table's frame encloses its own cells,
            // and an armed insert means "put something here", not "select".
            var onText = content.TextRuns.Any(run => Contains(BoundsOf(run), point));
            if (_contentInsertKind is null && !onText && BeginContentPathGesture(slot, sessionId, content, point, args))
            {
                return;
            }
            if (_selectedContentPath is not null)
            {
                ClearContentPathSelection();
                RedrawContentPathSelection();
            }
            // An armed insert kind turns a miss into new content; an image
            // under the pointer still wins, exactly as with nothing armed.
            if (_contentInsertKind is { } insert)
            {
                await InsertContentAtAsync(insert, (uint)pageIndex, point);
                return;
            }
            // A press on text is a click or a block drag; which one is decided
            // by how far the pointer goes before it lets go.
            if (await BeginTextBlockGestureAsync(slot, pageIndex, sessionId, content, point, args))
            {
                return;
            }
            await OpenContentEditorAsync((uint)pageIndex, point);
            return;
        }

        if (!ReferenceEquals(hit, _selectedContentImage))
        {
            _ = ProbeImageReplacementAsync(hit);
        }
        ClearContentPathSelection();
        _selectedContentImage = hit;
        ClaimPageFocus();
        // An image moved or resized before a save can be dragged again: the
        // core folds the new box into the queued edit, which keeps the
        // snapshot the file holds. Only Replace still waits for a save.
        _contentImageDrag = new(sessionId, hit, point, point, ReferenceEquals(hit, selected) ? corner : null);
        // No pointer in the smoke harness, which drives the gesture by points.
        if (args is not null) slot.Annotations.CapturePointer(args.Pointer);
        RedrawContentImageSelection();
    }

    private void ContinueContentImageGesture(int pageIndex, AnnotationPoint point)
    {
        if (_contentImageDrag is { Image.PageIndex: var dragPage } drag && dragPage == pageIndex)
        {
            _contentImageDrag = drag with { Current = point };
            RedrawContentImageSelection();
        }
    }

    private async Task EndContentImageGestureAsync(PageSlot slot, int pageIndex, Microsoft.UI.Xaml.Input.PointerRoutedEventArgs args)
    {
        if (_contentImageDrag is not { Image.PageIndex: var dragPage } drag || dragPage != pageIndex)
        {
            return;
        }

        var completed = drag with { Current = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position) };
        _contentImageDrag = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        await RecordContentImageDragAsync(completed);
    }

    /// <summary>Records the drag in progress as ending at <paramref name="point"/> — the smoke harness's release.</summary>
    private async Task CompleteContentImageDragAsync(AnnotationPoint point)
    {
        if (_contentImageDrag is not { } drag) return;
        _contentImageDrag = null;
        await RecordContentImageDragAsync(drag with { Current = point });
    }

    private async Task RecordContentImageDragAsync(ContentImageDrag completed)
    {
        var before = completed.Image.Bounds;
        var after = ImageDragRect(completed);
        if (before == after)
        {
            RedrawContentImageSelection();
            return;
        }

        if (_session?.SessionId != completed.SessionId || !_contentEditMode || !ContentEditButton.IsEnabled || _dialogOpen
            || !ReferenceEquals(_selectedContentImage, completed.Image))
        {
            RedrawContentImageSelection();
            return;
        }
        SetBusy(true);
        try
        {
            // The handle may predate an edit on another page; see CurrentContentImageAsync.
            if (await CurrentContentImageAsync(completed.SessionId, completed.Image) is not { } current)
            {
                return;
            }
            var result = await _facade.SetImageBoundsAsync(completed.SessionId, current, after);
            if (_session?.SessionId != completed.SessionId)
            {
                return;
            }

            // The facade records exactly one ResizeImage command for bounds
            // changes, including a corner drag that moved the origin.
            _contentEditedPages.Add(completed.Image.PageIndex);
            AfterImageEdit(completed.Image.PageIndex);
            if (!result.IsSuccess)
            {
                await RefreshAnnotationStateAsync();
                AnnotationStatus.Text = result.Error!.Message;
                return;
            }

            _pendingImageEdits.Add((completed.Image.PageIndex, completed.Image.Id));
            _annotationState = result.Value;
            UpdateAnnotationControls(_annotationState);
            AnnotationStatus.Text = completed.Corner is null ? "Image moved. Save to keep the change."
                : "Image resized. Save to keep the change.";
            // The selection frame follows the image: re-read the page and
            // select the same image where it now is, so the next drag starts
            // from what is painted.
            if (await EnsurePageContentAsync(completed.Image.PageIndex) is { } content
                && ReferenceEquals(_selectedContentImage, completed.Image))
            {
                _selectedContentImage = content.Images.FirstOrDefault(image => image.Id == completed.Image.Id);
            }
        }
        finally
        {
            SetBusy(false);
            RedrawContentImageSelection();
        }
    }

    private static AnnotationRect ImageDragRect(ContentImageDrag drag) => drag.Corner is { } corner
        ? ResizedRect(drag.Image.Bounds, corner, drag.Current)
        : MovedRect(drag.Image.Bounds, drag.Origin, drag.Current);

    private void CancelContentImageGesture(int pageIndex)
    {
        if (_contentImageDrag?.Image.PageIndex == pageIndex)
        {
            _contentImageDrag = null;
            RedrawContentImageSelection();
        }
    }

    private void ResetContentImageGestureState()
    {
        ClearContentImageSelection();
        ClearContentPathSelection();
        _pendingImageEdits.Clear();
        _replaceRefusedImages.Clear();
    }

    private void ClearContentImageSelection()
    {
        _selectedContentImage = null;
        _contentImageDrag = null;
    }

    private void RedrawContentImageSelection()
    {
        UpdateImageCard();
        foreach (var index in Enumerable.Range(0, _slots.Count))
        {
            DrawContentOutlines((uint)index);
        }
    }

    private void DrawContentImageSelection(PageSlot slot, uint pageIndex)
    {
        if (_selectedContentImage is not { } image || image.PageIndex != pageIndex)
        {
            return;
        }

        var bounds = _contentImageDrag is { Image.Id: var id } drag && id == image.Id
            ? ImageDragRect(drag)
            : image.Bounds;
        var outline = new Rectangle
        {
            Stroke = HandleBrush,
            StrokeThickness = 2,
            IsHitTestVisible = false,
        };
        PlaceOverPage(outline, slot, (int)pageIndex, bounds, minSide: 2);
        slot.Content.Children.Add(outline);
        DrawContentImageHandles(slot, pageIndex, bounds);
    }

    private void DrawContentImageHandles(PageSlot slot, uint pageIndex, AnnotationRect bounds)
    {
        foreach (var corner in AllCorners)
        {
            var point = PlaceOnPage(slot, (int)pageIndex, CornerPoint(bounds, corner));
            var handle = new Rectangle { Width = HandleReachPx, Height = HandleReachPx, Fill = HandleBrush, IsHitTestVisible = false };
            Microsoft.UI.Xaml.Controls.Canvas.SetLeft(handle, point.Left - HandleReachPx / 2);
            Microsoft.UI.Xaml.Controls.Canvas.SetTop(handle, point.Top - HandleReachPx / 2);
            slot.Content.Children.Add(handle);
        }
    }
}
