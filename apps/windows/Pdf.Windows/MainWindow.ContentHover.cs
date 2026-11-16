using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// What the pointer rests on in Edit content mode: a thin outline two screen
/// pixels outside the text block, image or vector path a press there would
/// grab — the Linux shell's <c>content_edit::hover</c> (PR #567).
/// </summary>
/// <remarks>
/// The item comes from the SHIFT+click hit-test (<see cref="ContentMemberAt"/>),
/// so what is outlined is always what a press would take. It is read from the
/// page's already parsed content and never parses it: a pointer crossing a
/// page must not cost a content-stream parse per move. The rules live in
/// <see cref="ContentHover"/>; this half keeps the state and places the one
/// outline element through the page placement, so it holds on a rotated page
/// and at any zoom.
///
/// A band <see cref="ContentHover.BorderBandPx"/> wide either side of the
/// outline is the item's border: the page shows the move cursor there, and a
/// press there moves the whole item through the drag its kind already has —
/// never a marquee, even where the band reaches outside the item's own box.
/// The corners of a selected image or shape keep their resize handles.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>
    /// The quiet box every editable item wears while the mode is on — a thin
    /// grey dashed line, so a page full of items still reads as a page. The
    /// page is always painted white, so one grey serves both themes.
    /// </summary>
    private static readonly SolidColorBrush RestingOutlineBrush = new(global::Windows.UI.Color.FromArgb(255, 115, 115, 115));

    /// <summary>The item under the pointer, or <c>null</c>.</summary>
    private ContentHoverState? _contentHover;

    /// <summary>The one hover outline, moved between pages' content canvases as the pointer goes.</summary>
    private readonly Rectangle _contentHoverOutline = new() { Stroke = HandleBrush, StrokeThickness = 1, IsHitTestVisible = false };

    /// <summary>Whether the press content-edit mode claimed found an inline editor open, read before it committed it.</summary>
    private bool _contentPressEditorOpen;

    /// <summary>The page whose surface shows the move cursor, or <c>null</c>.</summary>
    private PageSlot? _moveCursorSlot;

    private InputCursor? _moveCursor;

    /// <summary>Three pixels on, two off: the resting outline of an item nobody is pointing at.</summary>
    private static Rectangle RestingOutline() =>
        new() { Stroke = RestingOutlineBrush, StrokeThickness = 1, StrokeDashArray = [3, 2], IsHitTestVisible = false };

    /// <summary>Whether a press is being dragged: the drag paints its own live box, so the outline stands down.</summary>
    private bool ContentDragging =>
        _textBlockDrag is not null || _contentImageDrag is not null || _contentPathDrag is not null || _selectionGesture is not null;

    /// <summary>Whether an inline editor sits over the page and owns the pointer.</summary>
    private bool ContentEditorOpen => _pump.Box is not null || _insertEditor is not null;

    private void ConnectContentHover(PageSlot slot, int pageIndex)
    {
        slot.Annotations.PointerMoved += (_, args) =>
        {
            if (_session is null || pageIndex >= _session.Pages.Count) return;
            TrackContentHover(slot, pageIndex, ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position));
        };
        slot.Annotations.PointerExited += (_, _) => LeaveContentHover((uint)pageIndex);
    }

    /// <summary>
    /// The item a SHIFT+click at <paramref name="point"/> would toggle: an
    /// image, then a whole text block, then a vector path.
    /// </summary>
    private ContentHoverMember? ContentMemberAt(PageContent content, AnnotationPoint point, double reach)
    {
        if (content.Images.LastOrDefault(candidate => Contains(candidate.Bounds, point)) is { } image) return ContentHoverMember.Of(image);
        if (ContentHitTest.TextRunAt(content.TextRuns, BoundsOf, point.X, point.Y) is { } run && content.BlockOf(run) is { } block)
            return ContentHoverMember.Of(block, DrawnBounds(block));
        return PathAt(content, point, reach) is { } path ? ContentHoverMember.Of(path) : null;
    }

    /// <summary>Whether <paramref name="member"/> carries resize handles on <paramref name="pageIndex"/>: the selected image or shape.</summary>
    private bool CarriesHandles(uint pageIndex, ContentHoverMember member) => member.Kind switch
    {
        ContentHoverKind.Image => _selectedContentImage is { } image && image.PageIndex == pageIndex && image.Id == member.Id,
        ContentHoverKind.Path => _selectedContentPath is { } path && path.PageIndex == pageIndex && path.Id == member.Id,
        _ => false,
    };

    /// <summary>Follows the pointer to <paramref name="point"/> on a page.</summary>
    private void TrackContentHover(PageSlot slot, int pageIndex, AnnotationPoint point)
    {
        var page = (uint)pageIndex;
        if (!ContentHover.MayHover(_contentEditMode && !_organizing, _contentInsertKind is not null, ContentEditorOpen,
                _hand.SpaceHeld || _hand.Dragging))
        {
            ClearContentHover();
            return;
        }
        if (ContentDragging)
        {
            ShowContentHover();
            return;
        }
        if (!_pageContent.TryGetValue(page, out var state) || state.Content is not { } content) return;
        var reach = HandleReachPx / slot.Scale;
        var hover = ContentHover.Resolve(page, at => ContentMemberAt(content, at, reach), _contentHover, point, slot.Scale, reach,
            member => CarriesHandles(page, member));
        if (hover == _contentHover) return;
        _contentHover = hover;
        ShowContentHover();
    }

    /// <summary>The pointer left a page: whatever it rested on there is no longer under it.</summary>
    private void LeaveContentHover(uint pageIndex)
    {
        if (_contentHover?.PageIndex == pageIndex) ClearContentHover();
    }

    /// <summary>Drops the hover and its outline — the mode went off, an insert kind or an editor took the pointer.</summary>
    private void ClearContentHover()
    {
        _contentHover = null;
        ShowContentHover();
    }

    /// <summary>The hover, when its page is laid out and nothing rules it out; a drag rules out only the outline.</summary>
    private ContentHoverState? ShownContentHover() =>
        _contentHover is { } hover && _contentEditMode && hover.PageIndex < _slots.Count && _session is not null
            && hover.PageIndex < _session.Pages.Count && !ContentEditorOpen
            ? hover : null;

    /// <summary>Places the hover outline where the hovered item is drawn now, or takes it off the page, and the move cursor with it.</summary>
    private void ShowContentHover()
    {
        ShowMoveCursor();
        var outline = _contentHoverOutline;
        if (ShownContentHover() is not { } hover || ContentDragging)
        {
            (outline.Parent as Panel)?.Children.Remove(outline);
            return;
        }
        var slot = _slots[(int)hover.PageIndex];
        if (!ReferenceEquals(outline.Parent, slot.Content))
        {
            (outline.Parent as Panel)?.Children.Remove(outline);
            slot.Content.Children.Add(outline);
        }
        PlaceOverPage(outline, slot, (int)hover.PageIndex, ContentHover.OutlineRect(hover.Member.Bounds, slot.Scale));
    }

    /// <summary>
    /// The move cursor on the hovered page while the pointer is on the border
    /// band. It stays through the drag that band starts: the surface holds
    /// the pointer capture, so it is the element the cursor is read from.
    /// </summary>
    private void ShowMoveCursor()
    {
        var wanted = ShownContentHover() is { } hover && ContentHover.ShowsMoveCursor(hover.Zone) ? _slots[(int)hover.PageIndex] : null;
        if (ReferenceEquals(wanted, _moveCursorSlot) || ProtectedCursorProperty is null) return;
        if (_moveCursorSlot is { } previous) ProtectedCursorProperty.SetValue(previous.Annotations, null);
        if (wanted is not null)
        {
            _moveCursor ??= InputSystemCursor.Create(InputSystemCursorShape.SizeAll);
            ProtectedCursorProperty.SetValue(wanted.Annotations, _moveCursor);
        }
        _moveCursorSlot = wanted;
    }

    /// <summary>
    /// The hovered item, when <paramref name="point"/> is on its border band
    /// and it is still on the page as <paramref name="content"/> has it now —
    /// same id, same box — picked from that content rather than from the
    /// snapshot the hover holds. Never while an inline editor is or was open
    /// for this press.
    /// </summary>
    private ContentHoverMember? ContentBorderMember(uint pageIndex, PageContent content, AnnotationPoint point, double reach)
    {
        if (_contentPressEditorOpen || ContentEditorOpen || pageIndex >= _slots.Count) return null;
        if (_contentHover is not { } hover || hover.PageIndex != pageIndex) return null;
        if (ContentHover.StillCurrent(content, hover.Member) is not { } member) return null;
        var zone = ContentHover.ZoneOf(member.Bounds, point, _slots[(int)pageIndex].Scale, reach, CarriesHandles(pageIndex, member));
        return zone == ContentHoverZone.Border ? member : null;
    }

    /// <summary>
    /// A press on the hovered item's border band: the whole item moves, each
    /// kind through its own drag. A text block released without moving is
    /// selected — a click there is aimed at the block, even off its box.
    /// </summary>
    private bool BeginContentBorderMove(PageSlot slot, uint pageIndex, string sessionId, PageContent content,
        AnnotationPoint point, PointerRoutedEventArgs? args)
    {
        if (ContentBorderMember(pageIndex, content, point, HandleReachPx / slot.Scale) is not { } member) return false;
        switch (member)
        {
            case { Image: { } image }:
                ClearTextBlockSelection();
                ClearContentPathSelection();
                _selectedContentImage = image;
                ClaimPageFocus();
                _contentImageDrag = new(sessionId, image, point, point, Corner: null);
                break;
            case { Path: { } path }:
                ClearTextBlockSelection();
                if (_selectedContentImage is not null) ClearContentImageSelection();
                _selectedContentPath = path;
                ClaimPageFocus();
                _contentPathDrag = new(sessionId, path, point, point, Corner: null);
                break;
            case { Block: { } block }:
                if (_selectedContentImage is not null) ClearContentImageSelection();
                ClearContentPathSelection();
                if (!_contentPointerDown)
                {
                    // Released before the page content arrived: a click.
                    RedrawContentImageSelection();
                    SelectTextBlock(block);
                    return true;
                }
                _textBlockDrag = new(sessionId, block, point, point, Dragging: false);
                break;
            default:
                return false;
        }
        // No pointer in the smoke harness, which drives the gesture by points.
        if (args is not null) slot.Annotations.CapturePointer(args.Pointer);
        RedrawContentImageSelection();
        UpdateShapeCard();
        return true;
    }
}
