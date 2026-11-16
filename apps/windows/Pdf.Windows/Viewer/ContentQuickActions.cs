using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>What Edit content has selected, in the order the Delete key asks: several items, a block, an image, a shape.</summary>
public enum QuickActionsSelection { Several, TextBlock, Image, Shape }

/// <summary>
/// The buttons the quick-action bar offers for a selection. <c>null</c> hides
/// a button that does not apply to it; otherwise each flag says whether it
/// may be pressed.
/// </summary>
public sealed record QuickActionsOffer(bool? Retype, bool? Replace, bool Cut, bool Copy, bool Delete);

/// <summary>
/// The floating quick-action bar beside an Edit content selection — the Linux
/// shell's <c>content_edit::quick_actions</c>, kept free of WinUI so the
/// rules are testable without a window.
/// </summary>
/// <remarks>
/// It offers only what this shell can already do, each through the handler
/// its existing door calls — F2, the Images card's Replace, Ctrl+X, Ctrl+C,
/// the Delete key — under the gate that door uses. Nothing here decides
/// anything about an edit.
/// </remarks>
public static class ContentQuickActions
{
    /// <summary>The gap between the selection's box and the bar, in screen pixels.</summary>
    public const double GapPx = 6;

    /// <summary>
    /// Whether the bar may show at all: the mode is on, nothing is armed to
    /// insert, no press is being dragged, no inline editor is open, and
    /// neither Organize nor a dialog has the window.
    /// </summary>
    public static bool MayShow(bool modeOn, bool insertArmed, bool dragging, bool editorOpen, bool organizing, bool dialogOpen) =>
        modeOn && !insertArmed && !dragging && !editorOpen && !organizing && !dialogOpen;

    /// <summary>
    /// What <paramref name="selection"/> offers. <paramref name="idle"/> is the
    /// clipboard's own gate (not busy, no dialog, no editor);
    /// <paramref name="editable"/> the document's content-edit permission; the
    /// image and shape flags are their cards' Replace and Delete buttons.
    /// </summary>
    public static QuickActionsOffer Offer(QuickActionsSelection selection, bool idle, bool editable,
        bool replaceEnabled, bool imageDeleteEnabled, bool shapeDeleteEnabled)
    {
        var changes = idle && editable;
        return selection switch
        {
            QuickActionsSelection.TextBlock => new(Retype: changes, Replace: null, Cut: changes, Copy: idle, Delete: changes),
            QuickActionsSelection.Image => new(Retype: null, Replace: idle && replaceEnabled, Cut: changes, Copy: idle,
                Delete: idle && imageDeleteEnabled),
            QuickActionsSelection.Shape => new(Retype: null, Replace: null, Cut: changes, Copy: idle, Delete: idle && shapeDeleteEnabled),
            _ => new(Retype: null, Replace: null, Cut: changes, Copy: idle, Delete: changes),
        };
    }

    /// <summary>
    /// Where a bar of <paramref name="width"/> × <paramref name="height"/> goes
    /// beside <paramref name="anchor"/> (the selection's box) on a canvas of
    /// the given size, all in canvas pixels — or <c>null</c> when the
    /// selection is scrolled entirely out of view.
    /// </summary>
    /// <remarks>
    /// Right of the selection's top-right corner, top edges level; left of
    /// the selection when that would run off the canvas; above it — or below,
    /// with no room above — when neither side has room. Then clamped onto the
    /// canvas, so a selection running off an edge keeps its bar on screen.
    /// </remarks>
    public static (double X, double Y)? BarOrigin(PlacedRect anchor, double width, double height, double canvasWidth, double canvasHeight)
    {
        var right = anchor.Left + anchor.Width;
        var bottom = anchor.Top + anchor.Height;
        if (right < 0 || anchor.Left > canvasWidth || bottom < 0 || anchor.Top > canvasHeight) return null;

        var (x, y) = (right + GapPx, anchor.Top);
        if (x + width > canvasWidth)
        {
            x = anchor.Left - GapPx - width;
            if (x < 0)
            {
                x = right - width;
                y = anchor.Top - GapPx - height;
                if (y < 0) y = bottom + GapPx;
            }
        }
        return (Math.Max(Math.Min(x, canvasWidth - width), 0), Math.Max(Math.Min(y, canvasHeight - height), 0));
    }
}
