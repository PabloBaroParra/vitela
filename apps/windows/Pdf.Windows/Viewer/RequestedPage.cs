namespace Pdf.Windows.Viewer;

/// <summary>
/// The page the reader explicitly asked for (page list, Go to page,
/// Previous/Next), held until the scroller has moved somewhere else.
///
/// The viewport alone cannot answer "which page is current" after such a
/// request, for two reasons. <c>ChangeView</c> applies on the next layout
/// pass, so a viewport read straight after it still sees the old offset and
/// reports the page the reader just left. And the last pages of a document
/// cannot be scrolled to the top at all: the scroll stops at the end, an
/// earlier page sits under the fold, and the page list would snap the
/// selection to that page instead of the one that was picked.
/// </summary>
public sealed class RequestedPage
{
    /// <summary>Layout rounding between the offset asked for and the one the scroller reports.</summary>
    private const double Tolerance = 1.0;

    private int? _page;
    private double _offset;
    private bool _landed;

    /// <summary>Records the request and returns the offset to scroll to.</summary>
    public double Request(int page, double pageTop, double scrollableHeight)
    {
        _page = page;
        _offset = Math.Max(0, Math.Min(pageTop, scrollableHeight));
        _landed = false;
        return _offset;
    }

    /// <summary>
    /// The scroller settled. The first settle after a request is where that
    /// request ended up; later ones are the reader's own scrolling.
    /// </summary>
    public void Landed(double verticalOffset)
    {
        if (_page is null || _landed) return;
        _offset = verticalOffset;
        _landed = true;
    }

    /// <summary>Drops the request: the page boxes it was measured against are gone.</summary>
    public void Clear() => _page = null;

    /// <summary>
    /// The current page: the requested one while the scroll is on its way or
    /// still where the request left it, otherwise the first visible page.
    /// </summary>
    public int Current(int firstVisible, double verticalOffset)
    {
        if (_page is not { } page) return firstVisible;
        if (Math.Abs(verticalOffset - _offset) < Tolerance)
        {
            _landed = true;
            return page;
        }

        if (!_landed) return page;
        _page = null;
        return firstVisible;
    }
}
