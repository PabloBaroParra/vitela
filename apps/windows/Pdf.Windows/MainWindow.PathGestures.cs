using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Canvas selection and geometry gestures for vector paths (B25 T-206) — the
/// lines and boxes a table is drawn with. Drag to move, a corner to reshape,
/// Delete to remove.
/// </summary>
/// <remarks>
/// Text outranks a path under the pointer: a table frame encloses its own
/// text, and a click on a cell has to keep retyping that cell. An outline is
/// therefore hit only near its edges, and a filled shape only where no run
/// sits on top of it.
///
/// Unlike images there is no "save and reopen first" rule. The core reads a
/// pending move back at its new box, so the selection is re-read after every
/// edit and a second drag works straight away.
/// </remarks>
public sealed partial class MainWindow
{
    private sealed record ContentPathDrag(string SessionId, ContentPath Path, AnnotationPoint Origin,
        AnnotationPoint Current, Corner? Corner);

    private const string NoShapeSelectedHint = "Click a line or the border of a box to select it.";
    private const string ShapeSelectedHint = "Shape selected — drag it, drag a corner to resize, or press Delete.";

    private static readonly SolidColorBrush PathOutlineBrush = new(global::Windows.UI.Color.FromArgb(110, 30, 150, 90));

    private ContentPath? _selectedContentPath;
    private ContentPathDrag? _contentPathDrag;
    /// <summary>
    /// Where the selected path was dropped, while the core records the edit and
    /// rebuilds the preview. The frame stays there instead of snapping back to
    /// the old box for the length of the round trip.
    /// </summary>
    private AnnotationRect? _pathEditPreview;
    private bool _editingPath;
    private readonly Button _deleteShapeButton = new() { IsEnabled = false };
    private readonly TextBlock _editShapeHint = EditHint(NoShapeSelectedHint);

    /// <summary>The Shapes card, built beside the Images card.</summary>
    private Border BuildShapesCard()
    {
        ToolbarIcon(_deleteShapeButton, ShellIcon.Delete, "Delete shape", "Remove the selected line or box from the page",
            "Delete shape", EditIconSize, IconTint.Accent);
        _deleteShapeButton.Style = NamedStyle("EditTileButtonStyle");
        _deleteShapeButton.Click += async (_, _) => await DeleteSelectedPathAsync();
        var delete = new KeyboardAccelerator { Key = global::Windows.System.VirtualKey.Delete };
        delete.Invoked += async (_, args) =>
        {
            if (_selectedContentPath is null || !_deleteShapeButton.IsEnabled) return;
            args.Handled = true;
            await DeleteSelectedPathAsync();
        };
        _deleteShapeButton.KeyboardAccelerators.Add(delete);
        return EditCard("Shapes", "EditShapesCard", _editShapeHint, _deleteShapeButton);
    }

    private void UpdateShapeCard()
    {
        var selected = _session is not null && ContentEditButton.IsEnabled && _contentEditMode && _selectedContentPath is not null;
        _deleteShapeButton.IsEnabled = selected && !_isBusy && !_editingPath;
        _editShapeHint.Text = selected ? ShapeSelectedHint : NoShapeSelectedHint;
    }

    /// <summary>
    /// The path a press lands on, or <c>null</c>. The selected path keeps the
    /// press while it is under the pointer, so a corner handle that overhangs a
    /// neighbouring line still resizes the one with the handles.
    /// </summary>
    private ContentPath? PathAt(PageContent content, AnnotationPoint point, double reach)
    {
        if (_selectedContentPath is { } selected && selected.PageIndex == content.PageIndex
            && (CornerAt(selected.Bounds, point, reach) is not null || PathHit(selected, point, reach)))
        {
            return selected;
        }
        // Smallest first: a rule inside a frame inside a shaded panel is the
        // one a reader aiming at it means.
        return content.Paths
            .Where(path => PathHit(path, point, reach))
            .OrderBy(path => path.Bounds.Width * path.Bounds.Height)
            .FirstOrDefault();
    }

    private static bool PathHit(ContentPath path, AnnotationPoint point, double reach)
    {
        var bounds = path.Bounds;
        var padded = new AnnotationRect(bounds.X - reach, bounds.Y - reach, bounds.Width + 2 * reach, bounds.Height + 2 * reach);
        if (!Contains(padded, point)) return false;
        if (!path.IsOutline) return true;
        // An outline is its edges: inside the frame by more than the reach is
        // the cell, not the border.
        var inner = new AnnotationRect(bounds.X + reach, bounds.Y + reach, bounds.Width - 2 * reach, bounds.Height - 2 * reach);
        return inner.Width <= 0 || inner.Height <= 0 || !Contains(inner, point);
    }

    /// <summary>Starts a path gesture, or returns false when there is no path under the press.</summary>
    private bool BeginContentPathGesture(PageSlot slot, string sessionId, PageContent content, AnnotationPoint point,
        PointerRoutedEventArgs? args)
    {
        var reach = HandleReachPx / slot.Scale;
        if (PathAt(content, point, reach) is not { } hit)
        {
            if (_selectedContentPath is not null)
            {
                _selectedContentPath = null;
                RedrawContentPathSelection();
            }
            return false;
        }

        var corner = ReferenceEquals(hit, _selectedContentPath) ? CornerAt(hit.Bounds, point, reach) : null;
        _selectedContentPath = hit;
        _contentPathDrag = new(sessionId, hit, point, point, corner);
        // No pointer in the smoke harness, which drives the gesture by points.
        if (args is not null) slot.Annotations.CapturePointer(args.Pointer);
        RedrawContentPathSelection();
        return true;
    }

    private void ContinueContentPathGesture(int pageIndex, AnnotationPoint point)
    {
        if (_contentPathDrag is { Path.PageIndex: var dragPage } drag && dragPage == pageIndex)
        {
            _contentPathDrag = drag with { Current = point };
            RedrawContentPathSelection();
        }
    }

    private async Task EndContentPathGestureAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_contentPathDrag is not { Path.PageIndex: var dragPage } drag || dragPage != pageIndex)
        {
            return;
        }

        var completed = drag with { Current = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position) };
        // Cleared before the release, never after: releasing raises
        // PointerCaptureLost synchronously, and its cancel would otherwise
        // throw away the very drag being finished.
        _contentPathDrag = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        await RecordContentPathDragAsync(completed);
    }

    /// <summary>Records the drag in progress as ending at <paramref name="point"/>.</summary>
    private async Task CompleteContentPathDragAsync(AnnotationPoint point)
    {
        if (_contentPathDrag is not { } drag) return;
        _contentPathDrag = null;
        await RecordContentPathDragAsync(drag with { Current = point });
    }

    private async Task RecordContentPathDragAsync(ContentPathDrag completed)
    {
        var after = PathDragRect(completed);
        if (completed.Path.Bounds == after || _session?.SessionId != completed.SessionId || !_contentEditMode
            || !ContentEditButton.IsEnabled || _dialogOpen || !ReferenceEquals(_selectedContentPath, completed.Path))
        {
            RedrawContentPathSelection();
            return;
        }

        _editingPath = true;
        _pathEditPreview = after;
        SetBusy(true);
        RedrawContentPathSelection();
        try
        {
            if (await CurrentContentPathAsync(completed.SessionId, completed.Path) is not { } current) return;
            var result = completed.Corner is null
                ? await _facade.MovePathAsync(completed.SessionId, current, after.X, after.Y)
                : await _facade.SetPathBoundsAsync(completed.SessionId, current, after);
            if (_session?.SessionId != completed.SessionId) return;
            await AfterPathEditAsync(completed.Path, result,
                completed.Corner is null ? "Shape moved. Save to keep the change." : "Shape resized. Save to keep the change.");
        }
        finally
        {
            _editingPath = false;
            _pathEditPreview = null;
            SetBusy(false);
            RedrawContentPathSelection();
        }
    }

    private async Task DeleteSelectedPathAsync()
    {
        if (_session is null || _organizing || _isBusy || _editingPath || _selectedContentPath is not { } selected) return;
        var sessionId = _session.SessionId;
        _editingPath = true;
        UpdateShapeCard();
        try
        {
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != sessionId || !ReferenceEquals(_selectedContentPath, selected)) return;
            if (await CurrentContentPathAsync(sessionId, selected) is not { } current) return;
            var result = await _facade.RemovePathAsync(sessionId, current);
            if (_session?.SessionId != sessionId) return;
            _selectedContentPath = null;
            await AfterPathEditAsync(selected, result, "Shape deleted. Save to keep the change.");
        }
        finally
        {
            _editingPath = false;
            RedrawContentPathSelection();
        }
    }

    /// <summary>
    /// Re-renders the edited page and re-selects the path at its new box, so
    /// the next drag starts from what is now painted.
    /// </summary>
    private async Task AfterPathEditAsync(ContentPath edited, OperationResult<AnnotationState> result, string done)
    {
        // An edit remains undoable even if rebuilding its preview failed.
        _contentEditedPages.Add(edited.PageIndex);
        AfterImageEdit(edited.PageIndex);
        if (!result.IsSuccess)
        {
            _selectedContentPath = null;
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        AnnotationStatus.Text = done;
        if (_selectedContentPath is not null && await EnsurePageContentAsync(edited.PageIndex) is { } content)
        {
            _selectedContentPath = content.Paths.FirstOrDefault(path => path.Id == edited.Id);
        }
    }

    /// <summary>
    /// The selected path as the document stands now — the same re-read
    /// <see cref="CurrentContentImageAsync"/> does, for the same reason: an
    /// edit anywhere bumps the revision every snapshot is bound to.
    /// </summary>
    private async Task<ContentPath?> CurrentContentPathAsync(string sessionId, ContentPath selected)
    {
        var paths = await _facade.PagePathsAsync(sessionId, selected.PageIndex);
        if (_session?.SessionId != sessionId) return null;
        if (!paths.IsSuccess)
        {
            AnnotationStatus.Text = paths.Error!.Message;
            return null;
        }
        var current = paths.Value!.FirstOrDefault(path => path.Id == selected.Id && path.Bounds == selected.Bounds);
        if (current is null) AnnotationStatus.Text = "The page changed — select the shape again.";
        return current;
    }

    /// <summary>
    /// The box a drag produces. An axis the path is flat along (a rule's
    /// height) stays flat and in place: the core cannot stretch nothing, and
    /// the preview should not promise it will.
    /// </summary>
    private static AnnotationRect PathDragRect(ContentPathDrag drag)
    {
        var bounds = drag.Path.Bounds;
        if (drag.Corner is not { } corner) return MovedRect(bounds, drag.Origin, drag.Current);
        var resized = ResizedRect(bounds, corner, drag.Current);
        return new AnnotationRect(
            bounds.Width == 0 ? bounds.X : resized.X,
            bounds.Height == 0 ? bounds.Y : resized.Y,
            bounds.Width == 0 ? 0 : resized.Width,
            bounds.Height == 0 ? 0 : resized.Height);
    }

    private void CancelContentPathGesture(int pageIndex)
    {
        if (_contentPathDrag?.Path.PageIndex == pageIndex)
        {
            _contentPathDrag = null;
            RedrawContentPathSelection();
        }
    }

    private void ClearContentPathSelection()
    {
        _selectedContentPath = null;
        _contentPathDrag = null;
        UpdateShapeCard();
    }

    private void RedrawContentPathSelection()
    {
        UpdateShapeCard();
        foreach (var index in Enumerable.Range(0, _slots.Count))
        {
            DrawContentOutlines((uint)index);
        }
    }

    /// <summary>Faint outlines for every path, so a reader can see what is selectable, then the selection.</summary>
    private void DrawContentPaths(PageSlot slot, uint pageIndex, PageContent content)
    {
        foreach (var path in content.Paths)
        {
            if (ReferenceEquals(path, _selectedContentPath)) continue;
            var outline = new Rectangle { Stroke = PathOutlineBrush, StrokeThickness = 1, StrokeDashArray = [1, 2], IsHitTestVisible = false };
            PlaceOverPage(outline, slot, (int)pageIndex, path.Bounds, minSide: 2);
            slot.Content.Children.Add(outline);
        }

        if (_selectedContentPath is not { } selected || selected.PageIndex != pageIndex) return;
        var bounds = _contentPathDrag is { } drag && ReferenceEquals(drag.Path, selected) ? PathDragRect(drag)
            : _pathEditPreview ?? selected.Bounds;
        var frame = new Rectangle { Stroke = HandleBrush, StrokeThickness = 2, IsHitTestVisible = false };
        PlaceOverPage(frame, slot, (int)pageIndex, bounds, minSide: 2);
        slot.Content.Children.Add(frame);
        DrawContentImageHandles(slot, pageIndex, bounds);
    }
}
