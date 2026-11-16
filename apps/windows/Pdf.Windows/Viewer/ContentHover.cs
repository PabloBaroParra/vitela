using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Which kind of page content a hover rests on.</summary>
public enum ContentHoverKind { Text, Image, Path }

/// <summary>Which part of the hovered item the pointer is on.</summary>
public enum ContentHoverZone
{
    /// <summary>Inside the item, clear of its border: a press does what it always has.</summary>
    Inside,
    /// <summary>On the band round the outline: a press moves the whole item.</summary>
    Border,
    /// <summary>On a resize handle of the selected image or shape, which outranks the border it overlaps.</summary>
    Handle,
}

/// <summary>
/// One item a press could grab: a whole text block, an image or a vector path,
/// with the box it is drawn with. Exactly one of <see cref="Block"/>,
/// <see cref="Image"/> and <see cref="Path"/> is set, matching <see cref="Kind"/>.
/// </summary>
public sealed record ContentHoverMember(ContentHoverKind Kind, ulong Id, AnnotationRect Bounds,
    ContentTextBlock? Block = null, ContentImage? Image = null, ContentPath? Path = null)
{
    /// <summary>A text block, boxed the way it is outlined (its own estimate for a lone, just-edited run).</summary>
    public static ContentHoverMember Of(ContentTextBlock block, AnnotationRect drawn) =>
        new(ContentHoverKind.Text, block.First.Id, drawn, Block: block);

    public static ContentHoverMember Of(ContentImage image) => new(ContentHoverKind.Image, image.Id, image.Bounds, Image: image);

    public static ContentHoverMember Of(ContentPath path) => new(ContentHoverKind.Path, path.Id, path.Bounds, Path: path);
}

/// <summary>The item under the pointer, the page it was found on, and which part of it the pointer is on.</summary>
public sealed record ContentHoverState(uint PageIndex, ContentHoverMember Member, ContentHoverZone Zone);

/// <summary>
/// What the pointer rests on in Edit content mode: the thin outline round the
/// item a press would grab, and the border band round it where a press moves
/// the whole item — the Linux shell's <c>content_edit::hover</c>, kept free of
/// WinUI so the rules are testable without a window.
/// </summary>
/// <remarks>
/// Everything is worked out in PDF page space. The padding and the band are
/// screen distances, divided by the zoom on the way in; a page turns only by
/// quarter turns, which keep every distance, so rotation needs no say here —
/// the outline is placed through the page's placement like every overlay.
/// </remarks>
public static class ContentHover
{
    /// <summary>How far the outline sits outside the item's box, in screen pixels.</summary>
    public const double OutlinePadPx = 2;

    /// <summary>How far either side of the outline the border reaches, in screen pixels.</summary>
    public const double BorderBandPx = 4;

    /// <summary><paramref name="rect"/> grown by <paramref name="by"/> on every side.</summary>
    public static AnnotationRect Inflate(AnnotationRect rect, double by) =>
        new(rect.X - by, rect.Y - by, rect.Width + 2 * by, rect.Height + 2 * by);

    /// <summary>The outline's box in page space: the item's, padded by <see cref="OutlinePadPx"/> at <paramref name="scale"/> pixels per point.</summary>
    public static AnnotationRect OutlineRect(AnnotationRect bounds, double scale) => Inflate(bounds, OutlinePadPx / UsableScale(scale));

    /// <summary>How far <paramref name="point"/> is from the nearest edge of <paramref name="rect"/>, from inside or outside alike.</summary>
    public static double EdgeDistance(AnnotationRect rect, AnnotationPoint point)
    {
        var (left, right, bottom, top) = (rect.X, rect.X + rect.Width, rect.Y, rect.Y + rect.Height);
        var (x, y) = (point.X, point.Y);
        if (x >= left && x <= right && y >= bottom && y <= top)
        {
            return Math.Min(Math.Min(x - left, right - x), Math.Min(y - bottom, top - y));
        }
        var dx = Math.Max(Math.Max(left - x, 0), x - right);
        var dy = Math.Max(Math.Max(bottom - y, 0), y - top);
        return Math.Sqrt(dx * dx + dy * dy);
    }

    /// <summary>Where <paramref name="point"/> falls against an item boxed by <paramref name="bounds"/>: its border band, inside it, or clear of both.</summary>
    public static ContentHoverZone? Classify(AnnotationRect bounds, AnnotationPoint point, double scale)
    {
        var outline = OutlineRect(bounds, scale);
        if (EdgeDistance(outline, point) <= BorderBandPx / UsableScale(scale)) return ContentHoverZone.Border;
        return Contains(outline, point) ? ContentHoverZone.Inside : null;
    }

    /// <summary>
    /// The zone <paramref name="point"/> is in on an item; when the item
    /// <paramref name="carriesHandles"/>, a corner within <paramref name="reach"/>
    /// is its resize handle and outranks the band.
    /// </summary>
    public static ContentHoverZone? ZoneOf(AnnotationRect bounds, AnnotationPoint point, double scale, double reach, bool carriesHandles)
    {
        if (carriesHandles && NearCorner(bounds, point, reach)) return ContentHoverZone.Handle;
        return Classify(bounds, point, scale);
    }

    /// <summary>
    /// What the pointer at <paramref name="point"/> rests on.
    /// </summary>
    /// <remarks>
    /// <paramref name="memberAt"/> is the SHIFT+click hit-test, so what is
    /// outlined is always what a press there would take. When it finds
    /// nothing, the item already hovered (<paramref name="current"/>) holds
    /// while the pointer is on its border band — the band's outer half lies
    /// beyond the item's own box, where no hit-test reaches. A path the
    /// hit-test grabs within reach of its edges can lie beyond the band too;
    /// that still counts as its border. <paramref name="refind"/> lets the
    /// outer band count with nothing hovered, by probing a band's width away
    /// along each axis — how the hover is found again after an edit dropped
    /// it, without the pointer moving.
    /// </remarks>
    public static ContentHoverState? Resolve(uint pageIndex, Func<AnnotationPoint, ContentHoverMember?> memberAt,
        ContentHoverState? current, AnnotationPoint point, double scale, double reach,
        Func<ContentHoverMember, bool> carriesHandles, bool refind = false)
    {
        if (memberAt(point) is { } member)
        {
            var zone = ZoneOf(member.Bounds, point, scale, reach, carriesHandles(member)) ?? ContentHoverZone.Border;
            return new ContentHoverState(pageIndex, member, zone);
        }
        if (HeldOnBand(pageIndex, current, point, scale, reach, carriesHandles) is { } held) return held;
        if (!refind || current is not null) return null;
        var step = (OutlinePadPx + BorderBandPx + 1) / UsableScale(scale);
        AnnotationPoint[] probes =
        [
            new(point.X + step, point.Y), new(point.X - step, point.Y),
            new(point.X, point.Y + step), new(point.X, point.Y - step),
        ];
        foreach (var probe in probes)
        {
            if (memberAt(probe) is not { } near) continue;
            var candidate = new ContentHoverState(pageIndex, near, ContentHoverZone.Inside);
            if (HeldOnBand(pageIndex, candidate, point, scale, reach, carriesHandles) is { } found) return found;
        }
        return null;
    }

    /// <summary><paramref name="current"/>, still hovered while <paramref name="point"/> is on its border band or handle.</summary>
    public static ContentHoverState? HeldOnBand(uint pageIndex, ContentHoverState? current, AnnotationPoint point, double scale,
        double reach, Func<ContentHoverMember, bool> carriesHandles)
    {
        if (current is null || current.PageIndex != pageIndex) return null;
        var zone = ZoneOf(current.Member.Bounds, point, scale, reach, carriesHandles(current.Member));
        return zone is null or ContentHoverZone.Inside ? null : current with { Zone = zone.Value };
    }

    /// <summary>
    /// The hovered item as <paramref name="content"/> has it now — every piece
    /// with the same id and in the same place — or <c>null</c> when the page
    /// was parsed again underneath the hover. Content ids are positional, so a
    /// stale hover could otherwise name a different item.
    /// </summary>
    public static ContentHoverMember? StillCurrent(PageContent content, ContentHoverMember member)
    {
        switch (member.Kind)
        {
            case ContentHoverKind.Text when member.Block is { } block:
                var runs = block.Runs.Select(run => content.TextRuns.FirstOrDefault(now => now.Id == run.Id && now.Bounds == run.Bounds)).ToArray();
                if (runs.Length == 0 || runs.Any(run => run is null)) return null;
                return content.BlockOf(runs[0]!) is { } now && now.Runs.Count == block.Runs.Count
                    ? member with { Block = now } : null;
            case ContentHoverKind.Image when member.Image is { } image:
                return content.Images.FirstOrDefault(now => now.Id == image.Id && now.Bounds == image.Bounds) is { } currentImage
                    ? member with { Image = currentImage } : null;
            case ContentHoverKind.Path when member.Path is { } path:
                return content.Paths.FirstOrDefault(now => now.Id == path.Id && now.Bounds == path.Bounds) is { } currentPath
                    ? member with { Path = currentPath } : null;
            default:
                return null;
        }
    }

    /// <summary>Only the border moves an item as a whole, so only it changes the cursor.</summary>
    public static bool ShowsMoveCursor(ContentHoverZone? zone) => zone == ContentHoverZone.Border;

    /// <summary>
    /// Whether anything may be hovered at all: the mode is on, no insert kind
    /// is armed (every click places something new), no inline editor is open
    /// (it owns the pointer), and the hand tool is not panning.
    /// </summary>
    public static bool MayHover(bool modeOn, bool insertArmed, bool editorOpen, bool panning) =>
        modeOn && !insertArmed && !editorOpen && !panning;

    private static bool NearCorner(AnnotationRect rect, AnnotationPoint point, double reach)
    {
        foreach (var x in new[] { rect.X, rect.X + rect.Width })
        {
            foreach (var y in new[] { rect.Y, rect.Y + rect.Height })
            {
                if (Math.Abs(point.X - x) <= reach && Math.Abs(point.Y - y) <= reach) return true;
            }
        }
        return false;
    }

    private static bool Contains(AnnotationRect rect, AnnotationPoint point) =>
        point.X >= rect.X && point.X <= rect.X + rect.Width && point.Y >= rect.Y && point.Y <= rect.Y + rect.Height;

    /// <summary>A zoom that can be divided by; a page not laid out yet reports nothing sensible.</summary>
    private static double UsableScale(double scale) => double.IsFinite(scale) && scale > 0 ? scale : 1;
}
