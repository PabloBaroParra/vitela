namespace Pdf.Windows.Viewer;

/// <summary>
/// One collapsible editor side column (Pages on the left, Tools on the right):
/// whether it is open and the width the user chose for it.
/// </summary>
/// <remarks>
/// Mirrors the Linux shell's rule: a column is open exactly when its divider
/// leaves it room to draw. Dragging past the minimum folds it instead of
/// clipping its controls; dragging back across the minimum unfolds it, so the
/// divider gesture is its own inverse. The toggle reopens at the last open
/// width rather than a fixed default.
/// </remarks>
public sealed class SideColumn(double minimum, double maximum, double initial)
{
    private double _dragWidth;

    public double Minimum { get; } = minimum;

    public bool IsOpen { get; private set; } = true;

    /// <summary>The width the column draws at when nothing squeezes it.</summary>
    public double PreferredWidth { get; private set; } = Math.Clamp(initial, minimum, maximum);

    public void SetOpen(bool open) => IsOpen = open;

    /// <summary>Starts a divider drag from what is on screen now.</summary>
    public void BeginDrag() => _dragWidth = IsOpen ? PreferredWidth : 0;

    /// <summary>Widens (positive) or narrows (negative) the column by a drag step.</summary>
    public void DragBy(double widthDelta)
    {
        _dragWidth = Math.Max(0, _dragWidth + widthDelta);
        IsOpen = _dragWidth >= Minimum;
        if (IsOpen) PreferredWidth = Math.Min(_dragWidth, maximum);
    }

    /// <summary>
    /// The width to lay out when <paramref name="available"/> DIPs are left for
    /// this column. An open column never goes below its minimum; the preference
    /// survives a temporarily smaller window.
    /// </summary>
    public double Width(double available) =>
        IsOpen ? Math.Max(Minimum, Math.Min(PreferredWidth, available)) : 0;

    /// <summary>
    /// <see cref="Width(double)"/> for a narrow window: when <paramref name="available"/>
    /// is short the column may be squeezed below its minimum, down to
    /// <paramref name="squeezeFloor"/>, because the controls inside it wrap.
    /// </summary>
    public double Width(double available, double squeezeFloor) =>
        IsOpen ? Math.Max(Math.Min(squeezeFloor, Minimum), Math.Min(PreferredWidth, available)) : 0;

    /// <summary>
    /// The width to lay out, or 0 when <paramref name="available"/> cannot hold
    /// the column's minimum: Linux's rule that a column is open only while it
    /// has room to draw. The user's open/closed choice is untouched, so the
    /// column comes back when the window widens.
    /// </summary>
    public double WidthOrFold(double available) =>
        IsOpen && available >= Minimum ? Math.Min(PreferredWidth, available) : 0;
}
