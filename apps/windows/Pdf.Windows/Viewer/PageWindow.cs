namespace Pdf.Windows.Viewer;

/// <summary>One page's vertical placement in the stack, in DIPs.</summary>
public readonly record struct PageSpan(double Top, double Height)
{
    public double Bottom => Top + Height;
}

/// <summary>
/// The contiguous run of pages a viewport covers, plus the prefetch margin
/// around it. Kept apart from the WinUI page slots because it answers the one
/// question that decides whether the viewer feels fast: <em>is this page still
/// worth rendering?</em>
///
/// A render costs up to eight megapixels of PDFium work. Asking for one on
/// behalf of a page nobody is looking at does not just waste it — it competes
/// for the same cores as the page the reader is actually waiting on.
/// </summary>
public readonly record struct PageWindow(int First, int Last)
{
    /// <summary>Nothing to show: <see cref="Contains"/> is false for every index.</summary>
    public static PageWindow Empty => new(0, -1);

    public bool Contains(int index) => index >= First && index <= Last;

    /// <summary>
    /// The pages touching the viewport. A page cut by the top of the viewport
    /// is still on screen, so the run starts at the last page beginning at or
    /// above the fold, not at the first one fully below it.
    /// </summary>
    public static PageWindow Resolve(IReadOnlyList<PageSpan> pages, double viewportTop, double viewportHeight)
    {
        if (pages.Count == 0)
        {
            return Empty;
        }

        var first = LastPageStartingAtOrAbove(pages, viewportTop);
        if (first < 0 || pages[first].Bottom < viewportTop)
        {
            first = Math.Clamp(first + 1, 0, pages.Count - 1);
        }

        var viewportBottom = viewportTop + viewportHeight;
        var last = first;
        while (last + 1 < pages.Count && pages[last + 1].Top < viewportBottom)
        {
            last++;
        }

        return new PageWindow(first, last);
    }

    /// <summary>Widens the run by <paramref name="margin"/> pages on each side, bounded by the document.</summary>
    public PageWindow Expand(int margin, int pageCount) =>
        pageCount == 0 || Last < First
            ? Empty
            : new PageWindow(Math.Max(0, First - margin), Math.Min(pageCount - 1, Last + margin));

    /// <summary>
    /// Admits only visible pages while any of them still owes its current-DPI
    /// bitmap. Once they are settled, restores the usual prefetch margin.
    /// </summary>
    public PageWindow ForRenderRequests(Func<int, bool> needsRender, int prefetchMargin, int pageCount)
    {
        return HasOutstandingRender(needsRender) ? this : Expand(prefetchMargin, pageCount);
    }

    /// <summary>Whether any visible page still owes its current-DPI bitmap.</summary>
    public bool HasOutstandingRender(Func<int, bool> needsRender)
    {
        for (var index = First; index <= Last; index++)
        {
            if (needsRender(index))
            {
                return true;
            }
        }

        return false;
    }

    private static int LastPageStartingAtOrAbove(IReadOnlyList<PageSpan> pages, double viewportTop)
    {
        for (var index = pages.Count - 1; index >= 0; index--)
        {
            if (pages[index].Top <= viewportTop)
            {
                return index;
            }
        }

        return -1;
    }
}
