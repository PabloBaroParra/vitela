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

    /// <summary>
    /// Where the pointer last rested on a page, in page space, kept until it
    /// leaves that page — so a hover an edit dropped can be found again
    /// without the pointer moving — with the view it was read in: a scroll or
    /// zoom since slides the page under a pointer that stayed put, and the
    /// spot no longer names what is under it.
    /// </summary>
    private (uint Page, AnnotationPoint Point, (double X, double Y, double Scale) View)? _contentHoverPointer;

    /// <summary>The hover as it stood when the press content-edit mode claimed was made, read before any await.</summary>
    private ContentHoverState? _contentPressHover;

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

    /// <summary>
    /// Follows the pointer to <paramref name="point"/> on a page;
    /// <paramref name="refind"/> lets the outer band count with nothing
    /// hovered, for <see cref="RestoreContentHoverSoon"/>.
    /// </summary>
    private void TrackContentHover(PageSlot slot, int pageIndex, AnnotationPoint point, bool refind = false)
    {
        var page = (uint)pageIndex;
        _contentHoverPointer = (page, point, HoverView(slot));
        if (!ContentHover.MayHover(_contentEditMode && !_organizing, _contentInsertKind is not null, ContentEditorOpen,
                _hand.SpaceHeld || _hand.Dragging))
        {
            ClearContentHover();
            return;
        }
        if (ContentDragging)
        {
            // What is hovered cannot change mid-drag: only the outline has to
            // come off, once, and the item need not be looked up per move.
            if (_contentHoverOutline.Parent is not null) ShowContentHover();
            return;
        }
        if (!_pageContent.TryGetValue(page, out var state) || state.Content is not { } content) return;
        var reach = HandleReachPx / slot.Scale;
        var hover = ContentHover.Resolve(page, at => ContentMemberAt(content, at, reach), _contentHover, point, slot.Scale, reach,
            member => CarriesHandles(page, member), refind);
        if (hover == _contentHover) return;
        _contentHover = hover;
        ShowContentHover();
    }

    /// <summary>The pointer left a page: whatever it rested on there is no longer under it.</summary>
    private void LeaveContentHover(uint pageIndex)
    {
        if (_contentHoverPointer?.Page == pageIndex) _contentHoverPointer = null;
        if (_contentHover?.PageIndex == pageIndex) ClearContentHover();
    }

    /// <summary>
    /// Drops one page's parsed content — an edit was recorded on it, a step
    /// was undone or redone, the core wrote a checkpoint — and with it the
    /// hover, its outline and its move cursor. Content ids are positional, so
    /// after a re-parse the hover's ids could name different items; the next
    /// pointer move, or <see cref="RestoreContentHoverSoon"/>, finds it again.
    /// </summary>
    private void ForgetPageContent(uint pageIndex)
    {
        _pageContent.Remove(pageIndex);
        ClearContentHover();
    }

    /// <summary>
    /// Finds the hover again where the pointer last rested once
    /// <paramref name="pageIndex"/> was parsed again — so the outline and the
    /// move cursor follow an item just moved by its border, even by the outer
    /// band, without the pointer moving. Every gate of
    /// <see cref="TrackContentHover"/> still applies.
    /// </summary>
    private void RestoreContentHoverSoon(uint pageIndex)
    {
        if (_contentHoverPointer?.Page != pageIndex) return;
        DispatcherQueue.TryEnqueue(Microsoft.UI.Dispatching.DispatcherQueuePriority.Low, () =>
        {
            if (_contentHover is not null || _contentHoverPointer is not { } pointer || pointer.Page != pageIndex
                || _session is null || pageIndex >= _session.Pages.Count || pageIndex >= _slots.Count) return;
            var slot = _slots[(int)pageIndex];
            // Scrolled or zoomed since — by keyboard, say, which moves no
            // pointer — the spot is somewhere else on the page now.
            if (pointer.View != HoverView(slot)) return;
            TrackContentHover(slot, (int)pageIndex, pointer.Point, refind: true);
        });
    }

    /// <summary>The view a pointer spot on <paramref name="slot"/> was read in: the scroll offsets and the page's zoom.</summary>
    private (double X, double Y, double Scale) HoverView(PageSlot slot) =>
        (PageScroller.HorizontalOffset, PageScroller.VerticalOffset, slot.Scale);

    /// <summary>Drops the hover and its outline — the mode went off, an insert kind or an editor took the pointer.</summary>
    private void ClearContentHover()
    {
        _contentHover = null;
        ShowContentHover();
    }

    /// <summary>
    /// The hover, when its page is laid out and nothing rules it out; a drag
    /// rules out only the outline. A member of the held several-item
    /// selection shows neither box nor move cursor: the group frame is the
    /// only thing drawn round its items, as on Linux.
    /// </summary>
    private ContentHoverState? ShownContentHover() =>
        _contentHover is { } hover && _contentEditMode && hover.PageIndex < _slots.Count && _session is not null
            && hover.PageIndex < _session.Pages.Count && !ContentEditorOpen
            && !ContentHover.InHeldGroup(_contentSelection, hover.PageIndex, hover.Member) && IsCurrent(hover)
            ? hover : null;

    /// <summary>Whether the hovered item is still on its page as parsed now, the same id in the same place.</summary>
    private bool IsCurrent(ContentHoverState hover) =>
        _pageContent.TryGetValue(hover.PageIndex, out var state) && state.Content is { } content
        && ContentHover.StillCurrent(content, hover.Member) is not null;

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
    /// The item hovered when the press was made, when <paramref name="point"/>
    /// is on its border band, nothing else is under it
    /// (<paramref name="hit"/>), and it is still on the page as
    /// <paramref name="content"/> has it now — same id, same box — picked
    /// from that content rather than from the snapshot the hover holds.
    /// Never while an inline editor is or was open for this press.
    /// </summary>
    /// <remarks>
    /// The press-time hover, not the live one: the pointer keeps moving while
    /// the press waits for the page content, and a fast drag off the outer
    /// band would otherwise find nothing hovered and start a marquee.
    /// </remarks>
    private ContentHoverMember? ContentBorderMember(uint pageIndex, PageContent content, AnnotationPoint point, double reach,
        ContentHoverMember? hit)
    {
        if (_contentPressEditorOpen || ContentEditorOpen || pageIndex >= _slots.Count) return null;
        if (_contentPressHover is not { } hover || hover.PageIndex != pageIndex) return null;
        if (ContentHover.StillCurrent(content, hover.Member) is not { } member || !ContentHover.BorderMayClaim(hit, member)) return null;
        var zone = ContentHover.ZoneOf(member.Bounds, point, _slots[(int)pageIndex].Scale, reach, CarriesHandles(pageIndex, member));
        return zone == ContentHoverZone.Border ? member : null;
    }

    /// <summary>
    /// A press on the hovered item's border band: the whole item moves, each
    /// kind through its own drag. Released before the page content arrived,
    /// it is a click aimed at the item, even off its box: the item is
    /// selected and nothing is dragged.
    /// </summary>
    private bool BeginContentBorderMove(PageSlot slot, uint pageIndex, string sessionId, PageContent content,
        AnnotationPoint point, PointerRoutedEventArgs? args, ContentHoverMember? hit)
    {
        if (ContentBorderMember(pageIndex, content, point, HandleReachPx / slot.Scale, hit) is not { } member) return false;
        var held = _contentPointerDown;
        switch (member)
        {
            case { Image: { } image }:
                ClearTextBlockSelection();
                ClearContentPathSelection();
                _selectedContentImage = image;
                ClaimPageFocus();
                if (held) _contentImageDrag = new(sessionId, image, point, point, Corner: null);
                break;
            case { Path: { } path }:
                ClearTextBlockSelection();
                if (_selectedContentImage is not null) ClearContentImageSelection();
                _selectedContentPath = path;
                ClaimPageFocus();
                if (held) _contentPathDrag = new(sessionId, path, point, point, Corner: null);
                break;
            case { Block: { } block }:
                if (_selectedContentImage is not null) ClearContentImageSelection();
                ClearContentPathSelection();
                if (!held)
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
        if (held && args is not null) slot.Annotations.CapturePointer(args.Pointer);
        RedrawContentImageSelection();
        if (!held) RedrawContentPathSelection();
        UpdateShapeCard();
        return true;
    }
}
