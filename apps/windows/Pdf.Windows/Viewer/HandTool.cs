namespace Pdf.Windows.Viewer;

/// <summary>The mouse button behind a press, as far as the hand tool cares.</summary>
internal enum PanButton
{
    Primary,
    Middle,
    Other,
}

/// <summary>The cursor the hand tool calls for; <see cref="None"/> restores the scroller's own.</summary>
internal enum PanCursor
{
    None,
    /// <summary>Space is held over the document: a press would grab it.</summary>
    Grab,
    /// <summary>The document is following the pointer.</summary>
    Grabbing,
}

/// <summary>
/// Hand-tool panning rules, mirroring the GTK shell's <c>app/pan.rs</c>: hold
/// Space and drag with the primary button, or drag with the middle button, to
/// move the document under the pointer. No UI types, so the facade suite can
/// cover every transition; <c>MainWindow.Pan.cs</c> feeds it WinUI input and
/// writes the scroll offsets it returns.
/// </summary>
internal sealed class HandTool
{
    private bool _space;
    private Drag? _drag;

    /// <summary>Space is held and armed the hand.</summary>
    public bool SpaceHeld => _space;

    /// <summary>A press is claimed, live or not, until its button comes up.</summary>
    public bool Dragging => _drag is not null;

    public PanCursor Cursor => _drag is { Live: true } ? PanCursor.Grabbing
        : _space ? PanCursor.Grab
        : PanCursor.None;

    /// <summary>
    /// Space went down, or repeated. True swallows the key so nothing else
    /// acts on it. Checked on every press, autorepeats included: Space typed
    /// into a focused text field is text, even if the hand was armed a moment
    /// ago. A repeat keeps the hand armed even if the pointer drifted off.
    /// </summary>
    public bool PressSpace(bool textInputFocused, bool overDocument, bool hasPages)
    {
        if (textInputFocused)
        {
            Reset();
            return false;
        }
        if (!_space)
        {
            if (!overDocument || !hasPages) return false;
            _space = true;
        }
        return true;
    }

    /// <summary>
    /// Space came up. A primary-button drag stops moving the document but its
    /// press stays claimed until the button comes up, so the rest of it cannot
    /// reach the page tools. A middle-button drag keeps panning. True when the
    /// release belongs to a Space the hand had taken.
    /// </summary>
    public bool ReleaseSpace()
    {
        var armed = _space;
        _space = false;
        if (_drag is { NeedsSpace: true } drag) drag.Live = false;
        return armed;
    }

    /// <summary>
    /// A button went down over the document with the scroller at
    /// (<paramref name="startX"/>, <paramref name="startY"/>). True claims the
    /// press; false leaves it to the page tools. A press arriving while one is
    /// already claimed (the same press seen twice on its way up the tree)
    /// stays claimed without moving the drag's origin.
    /// </summary>
    public bool Begin(PanButton button, bool hasPages, double startX, double startY)
    {
        if (_drag is not null) return true;
        if (!Grabs(button, _space) || !hasPages) return false;
        _drag = new Drag(startX, startY, needsSpace: button != PanButton.Middle);
        return true;
    }

    /// <summary>
    /// The pointer moved (<paramref name="dx"/>, <paramref name="dy"/>) since
    /// the press. Returns the scroll offsets to show, or null when nothing
    /// should move. <paramref name="maxX"/>/<paramref name="maxY"/> are the
    /// scroller's scrollable extents.
    /// </summary>
    public (double X, double Y)? Update(double dx, double dy, double maxX, double maxY) =>
        _drag is { Live: true } drag
            ? (Panned(drag.StartX, dx, maxX), Panned(drag.StartY, dy, maxY))
            : null;

    /// <summary>The button came up, or the press was taken away.</summary>
    public void End() => _drag = null;

    /// <summary>Drop everything: the window lost focus, or a text field took Space.</summary>
    public void Reset()
    {
        _space = false;
        _drag = null;
    }

    /// <summary>
    /// Whether a press of <paramref name="button"/> starts a pan: the middle
    /// button always does, the primary one only while Space is held.
    /// Everything else, and the primary button without Space, belongs to the
    /// page tools.
    /// </summary>
    public static bool Grabs(PanButton button, bool spaceHeld) =>
        button == PanButton.Middle || (button == PanButton.Primary && spaceHeld);

    /// <summary>
    /// One axis' offset after the pointer moved <paramref name="offset"/> DIPs
    /// since the drag began at <paramref name="start"/>. The content follows
    /// the pointer, so dragging right or down lowers the offset. Clamped to
    /// [0, <paramref name="max"/>], collapsing to 0 when nothing scrolls. A
    /// non-finite offset leaves the axis where the drag started.
    /// </summary>
    public static double Panned(double start, double offset, double max)
    {
        var upper = double.IsFinite(max) ? Math.Max(0, max) : 0;
        var target = double.IsFinite(offset) ? start - offset : start;
        return Math.Clamp(target, 0, upper);
    }

    private sealed class Drag(double startX, double startY, bool needsSpace)
    {
        public double StartX { get; } = startX;
        public double StartY { get; } = startY;
        /// <summary>Started with the primary button, so it pans only while Space is held.</summary>
        public bool NeedsSpace { get; } = needsSpace;
        /// <summary>False once Space was let go mid-drag: still claimed, no longer panning.</summary>
        public bool Live { get; set; } = true;
    }
}
