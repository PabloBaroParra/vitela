using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>One line to draw, positioned in the box's upright frame, in display pixels.</summary>
public sealed record FreeTextDrawLine(string Text, double Left, double Top);

/// <summary>What the overlay paints for one FreeText box, in display pixels.</summary>
public sealed record FreeTextDrawPlan(double FontSizePx, double WidthPx, double HeightPx, IReadOnlyList<FreeTextDrawLine> Lines);

/// <summary>
/// Turns the core's laid-out lines into positions for a WinUI <c>TextBlock</c>
/// per line. The shell never wraps: it draws what the core says, so the
/// overlay, the saved appearance and every other shell cannot disagree about
/// where a line breaks.
/// </summary>
public static class FreeTextDraw
{
    /// <summary>
    /// How far below a <c>TextBlock</c>'s top its baseline sits, as a fraction
    /// of the font size, for the face the overlay uses (Arial's ascent, 0.905).
    /// The core hands over baselines; a <c>TextBlock</c> is placed by its top.
    /// </summary>
    public const double AscentEm = 0.905;

    /// <summary>Null when there is nothing the core vouches for: not a FreeText, no rect, or no layout.</summary>
    public static FreeTextDrawPlan? Plan(Annotation annotation, double scale)
    {
        if (annotation.Kind != AnnotationKind.FreeText || annotation.Rect is not { } rect || annotation.Layout is not { } layout) return null;
        var fontSizePx = layout.FontSizePt * scale;
        var lines = layout.Lines
            .Select(line => new FreeTextDrawLine(line.Text, line.XPt * scale, line.BaselineFromTopPt * scale - AscentEm * fontSizePx))
            .ToArray();
        return new FreeTextDrawPlan(fontSizePx, rect.Width * scale, rect.Height * scale, lines);
    }
}
