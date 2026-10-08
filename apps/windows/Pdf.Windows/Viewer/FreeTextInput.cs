using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>
/// The shell's own FreeText decisions, kept free of WinUI so they can be tested
/// without a window: when the dialog may say Add, where a click or a drag puts
/// the box, and how a resize respects the core's minimum. Everything about the
/// <em>text</em> — wrapping, encoding, validation — stays in the core.
/// </summary>
public static class FreeTextInput
{
    /// <summary>The box a plain click places, in points. The core does not own this; the shell passes the rect.</summary>
    public const double DefaultWidthPt = 200;
    public const double DefaultHeightPt = 50;

    /// <summary>
    /// The smallest box the core accepts for 12 pt text: one glyph wide, one
    /// line high, plus padding (16 x 17.8). The height is rounded up so a shell
    /// rect never lands on the refusal boundary.
    /// </summary>
    public const double MinWidthPt = 16;
    public const double MinHeightPt = 18;

    /// <summary>A drag shorter than this on both axes is a click, not a drawn box.</summary>
    public const double TapSlopPt = 4;

    /// <summary>The dialog's Add/Save button is live only for text with something visible in it.</summary>
    public static bool AddEnabled(string? text) => !string.IsNullOrWhiteSpace(text);

    /// <summary>
    /// Whether confirming the edit dialog should issue a command: a blank text
    /// is never sent, and the same text is not an edit (no undo entry). A change
    /// in whitespace alone is real — the core stores the text as typed.
    /// </summary>
    public static bool EditChangesText(string current, string? typed) =>
        AddEnabled(typed) && !string.Equals(current, typed, StringComparison.Ordinal);

    /// <summary>
    /// The rect a placement gesture produces, in PDF space (y grows upward).
    /// A click hangs the default box from the click point — its top-left is
    /// where the reader pointed — and a drag places the dragged rectangle,
    /// raised to the minimum. Either way the box stays on the page.
    /// </summary>
    public static AnnotationRect PlacementRect(AnnotationPoint origin, AnnotationPoint release, double pageWidthPt, double pageHeightPt)
    {
        var tap = Math.Abs(release.X - origin.X) < TapSlopPt && Math.Abs(release.Y - origin.Y) < TapSlopPt;
        double width, height, left, top;
        if (tap)
        {
            (width, height) = (DefaultWidthPt, DefaultHeightPt);
            (left, top) = (origin.X, origin.Y);
        }
        else
        {
            width = Math.Max(MinWidthPt, Math.Abs(release.X - origin.X));
            height = Math.Max(MinHeightPt, Math.Abs(release.Y - origin.Y));
            (left, top) = (Math.Min(origin.X, release.X), Math.Max(origin.Y, release.Y));
        }

        width = Math.Min(width, pageWidthPt);
        height = Math.Min(height, pageHeightPt);
        left = Math.Clamp(left, 0, pageWidthPt - width);
        top = Math.Clamp(top, height, pageHeightPt);
        return new AnnotationRect(left, top - height, width, height);
    }

    /// <summary>
    /// The rect a resize drag produces: the corner opposite the grabbed one
    /// stays at <paramref name="anchor"/>, the grabbed corner follows the
    /// pointer, and a side that would fall below the minimum stops there —
    /// growing away from the anchor, on whichever side the pointer is.
    /// </summary>
    public static AnnotationRect ResizedRect(AnnotationPoint anchor, AnnotationPoint pointer)
    {
        var width = Math.Max(MinWidthPt, Math.Abs(pointer.X - anchor.X));
        var height = Math.Max(MinHeightPt, Math.Abs(pointer.Y - anchor.Y));
        var left = pointer.X >= anchor.X ? anchor.X : anchor.X - width;
        var bottom = pointer.Y >= anchor.Y ? anchor.Y : anchor.Y - height;
        return new AnnotationRect(left, bottom, width, height);
    }

    /// <summary>
    /// A page's size in its own unrotated space — the space annotation rects
    /// live in. <see cref="PageDimensions"/> reports the drawn size, which a
    /// quarter turn has already swapped.
    /// </summary>
    public static (double Width, double Height) UnrotatedPageSize(PageDimensions page) =>
        page.Rotation is PageRotation.Clockwise90 or PageRotation.Clockwise270
            ? (page.HeightPt, page.WidthPt)
            : (page.WidthPt, page.HeightPt);
}
