namespace Pdf.Windows.Viewer;

/// <summary>
/// One page's render bookkeeping: the DPI the current zoom wants, the DPI the
/// bitmap on screen was actually made at, and whether a render is already in
/// flight. Kept apart from the WinUI page slot so the two rules that make
/// zooming feel immediate can be verified without a UI runtime:
///
/// <list type="number">
/// <item>a zoom change never clears the bitmap already on screen — it is
/// scaled into the new box and replaced once a sharper one arrives, so the
/// page never goes blank;</item>
/// <item>a render that lands after the zoom moved is discarded <em>and</em>
/// leaves the page asking again, instead of stranding it until the next
/// scroll event.</item>
/// </list>
/// </summary>
public sealed class PageRenderPlan
{
    /// <summary>DPI the current zoom wants for this page; 0 before the first layout.</summary>
    public uint TargetDpi { get; private set; }

    /// <summary>DPI the bitmap on screen was rendered at; 0 when there is none.</summary>
    public uint RenderedDpi { get; private set; }

    /// <summary>Whether a render for this page is in flight.</summary>
    public bool Requested { get; private set; }

    public bool HasBitmap => RenderedDpi != 0;

    /// <summary>
    /// True when what is on screen does not match the current zoom. A stale
    /// bitmap still counts as showing something, so this means "owes a
    /// sharper render", not "shows nothing".
    /// </summary>
    public bool NeedsRender => RenderedDpi != TargetDpi;

    /// <summary>
    /// DPI the last render failed at, latched for the current fit; 0 when
    /// none has. Mirrors Linux's <c>PageState::Failed</c>.
    /// </summary>
    private uint _failedDpi;

    /// <summary>
    /// True when the page still owes a render it can actually get. A page
    /// whose render failed at the current fit owes nothing until the next one.
    /// </summary>
    public bool OwesRender => NeedsRender && _failedDpi != TargetDpi;

    public bool ShouldRequest => OwesRender && !Requested;

    /// <summary>
    /// Points this page at the DPI the current fit calls for. Every new fit
    /// clears a failure latch, giving a failed page one retry — even at the
    /// same DPI, as Linux's <c>refresh_layout</c> does.
    /// </summary>
    public void RetargetTo(uint dpi)
    {
        TargetDpi = dpi;
        _failedDpi = 0;
    }

    public void MarkRequested() => Requested = true;

    /// <summary>
    /// Records a finished render.
    /// </summary>
    /// <returns>
    /// <c>true</c> when the bitmap matches the zoom that is current now and
    /// should be published; <c>false</c> when the zoom moved while it was
    /// rendering, in which case <see cref="ShouldRequest"/> is true again.
    /// </returns>
    public bool CompleteWith(uint dpi)
    {
        Requested = false;
        if (dpi != TargetDpi)
        {
            return false;
        }

        RenderedDpi = dpi;
        return true;
    }

    /// <summary>
    /// Releases a completed render that no longer matches the target before it
    /// is materialized into a bitmap.
    /// </summary>
    /// <returns><c>true</c> when the completed render is superseded.</returns>
    public bool DiscardIfSuperseded(uint dpi)
    {
        if (dpi == TargetDpi)
        {
            return false;
        }

        Requested = false;
        return true;
    }

    /// <summary>
    /// Records a render that failed, so the viewport walk stops re-queueing a
    /// job that can only fail again. A failure at a superseded DPI says nothing
    /// about the current one and is not latched.
    /// </summary>
    public void Fail(uint dpi)
    {
        Requested = false;
        if (dpi == TargetDpi)
        {
            _failedDpi = dpi;
        }
    }

    /// <summary>Releases the in-flight slot after a discarded render; nothing failed.</summary>
    public void Release() => Requested = false;

    /// <summary>Drops the bitmap when the page leaves the keep window.</summary>
    public void DropBitmap() => RenderedDpi = 0;
}
