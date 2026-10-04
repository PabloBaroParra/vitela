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
        if (hit is null)
        {
            if (_selectedContentImage is not null)
            {
                _selectedContentImage = null;
                RedrawContentImageSelection();
            }
            // An armed insert kind turns a miss into new content; an image
            // under the pointer still wins, exactly as with nothing armed.
            if (_contentInsertKind is { } insert)
            {
                await InsertContentAtAsync(insert, (uint)pageIndex, point);
                return;
            }
            await OpenContentEditorAsync((uint)pageIndex, point);
            return;
        }

        if (!ReferenceEquals(hit, _selectedContentImage))
        {
            _ = ProbeImageReplacementAsync(hit);
        }
        _selectedContentImage = hit;
        if (_pendingImageEdits.Contains((hit.PageIndex, hit.Id)))
        {
            AnnotationStatus.Text = "Save and reopen before editing this image again.";
            RedrawContentImageSelection();
            return;
        }

        _contentImageDrag = new(sessionId, hit, point, point, ReferenceEquals(hit, selected) ? corner : null);
        slot.Annotations.CapturePointer(args.Pointer);
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
        if (_pendingImageEdits.Contains((completed.Image.PageIndex, completed.Image.Id)))
        {
            AnnotationStatus.Text = "Save and reopen before editing this image again.";
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
