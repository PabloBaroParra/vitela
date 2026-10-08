using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// The shell-side rules of FreeText that need no window: when the dialog may
/// say Add, where a click or drag puts the box, how a resize keeps its
/// minimum, and what the overlay draws (the core's lines, never a re-wrap).
/// </summary>
internal static class FreeTextInputTests
{
    public static Task RunAsync()
    {
        DialogRules();
        Placement();
        Resize();
        PageSpace();
        DrawPlan();
        return Task.CompletedTask;
    }

    private static void DialogRules()
    {
        Check(!FreeTextInput.AddEnabled(null), "no text, no Add");
        Check(!FreeTextInput.AddEnabled(""), "an empty box cannot be added");
        Check(!FreeTextInput.AddEnabled(" \r\n\t "), "a whitespace-only box cannot be added");
        Check(FreeTextInput.AddEnabled("x"), "one character enables Add");
        Check(FreeTextInput.AddEnabled("  hola  "), "padded text enables Add");

        Check(!FreeTextInput.EditChangesText("uno", "uno"), "retyping the same text is not an edit");
        Check(FreeTextInput.EditChangesText("uno", "dos"), "different text is an edit");
        Check(!FreeTextInput.EditChangesText("uno", "   "), "a blank edit is refused, never sent");
        Check(FreeTextInput.EditChangesText("uno", "uno "), "trailing space is a real change the core stores");
    }

    private static void Placement()
    {
        // A tap puts the default box's top-left at the click (PDF space: y up, so y is the bottom).
        var tap = FreeTextInput.PlacementRect(new AnnotationPoint(100, 700), new AnnotationPoint(100, 700), 595, 842);
        Check(tap == new AnnotationRect(100, 650, 200, 50), $"tap places 200x50 hanging from the click, got {tap}");

        // Near the right edge the box slides left to stay on the page.
        var rightEdge = FreeTextInput.PlacementRect(new AnnotationPoint(550, 700), new AnnotationPoint(550, 700), 595, 842);
        Check(rightEdge == new AnnotationRect(395, 650, 200, 50), $"box is clamped to the right edge, got {rightEdge}");

        // Near the bottom it slides up.
        var bottom = FreeTextInput.PlacementRect(new AnnotationPoint(100, 10), new AnnotationPoint(100, 10), 595, 842);
        Check(bottom == new AnnotationRect(100, 0, 200, 50), $"box is clamped to the bottom edge, got {bottom}");

        // A click above the page top cannot push the box off it.
        var top = FreeTextInput.PlacementRect(new AnnotationPoint(100, 900), new AnnotationPoint(100, 900), 595, 842);
        Check(top == new AnnotationRect(100, 792, 200, 50), $"box is clamped to the top edge, got {top}");

        // A page smaller than the default box shrinks it.
        var tiny = FreeTextInput.PlacementRect(new AnnotationPoint(5, 40), new AnnotationPoint(5, 40), 120, 40);
        Check(tiny == new AnnotationRect(0, 0, 120, 40), $"box never exceeds the page, got {tiny}");

        // A real drag places the dragged rectangle, whichever way it was dragged.
        var drag = FreeTextInput.PlacementRect(new AnnotationPoint(100, 700), new AnnotationPoint(300, 600), 595, 842);
        Check(drag == new AnnotationRect(100, 600, 200, 100), $"a drag places its own rect, got {drag}");
        var backwards = FreeTextInput.PlacementRect(new AnnotationPoint(300, 600), new AnnotationPoint(100, 700), 595, 842);
        Check(backwards == drag, "dragging up-left gives the same rect");

        // A sliver is raised to the core's minimum, keeping the top-left.
        var sliver = FreeTextInput.PlacementRect(new AnnotationPoint(100, 700), new AnnotationPoint(106, 650), 595, 842);
        Check(sliver.Width == FreeTextInput.MinWidthPt && sliver.Height == 50 && sliver.X == 100 && sliver.Y == 650,
            $"a narrow drag is widened to the minimum, got {sliver}");
    }

    private static void Resize()
    {
        var anchor = new AnnotationPoint(100, 600); // bottom-left stays put (grabbed the top-right)
        var grown = FreeTextInput.ResizedRect(anchor, new AnnotationPoint(300, 700));
        Check(grown == new AnnotationRect(100, 600, 200, 100), $"plain resize, got {grown}");

        var flipped = FreeTextInput.ResizedRect(anchor, new AnnotationPoint(40, 560));
        Check(flipped == new AnnotationRect(40, 560, 60, 40), $"dragging past the anchor flips the rect, got {flipped}");

        var narrow = FreeTextInput.ResizedRect(anchor, new AnnotationPoint(105, 700));
        Check(narrow == new AnnotationRect(100, 600, FreeTextInput.MinWidthPt, 100), $"too narrow stops at the minimum, away from the anchor, got {narrow}");

        var narrowLeft = FreeTextInput.ResizedRect(anchor, new AnnotationPoint(95, 700));
        Check(narrowLeft == new AnnotationRect(100 - FreeTextInput.MinWidthPt, 600, FreeTextInput.MinWidthPt, 100),
            $"too narrow on the left side extends left of the anchor, got {narrowLeft}");

        var short1 = FreeTextInput.ResizedRect(anchor, new AnnotationPoint(300, 602));
        Check(short1 == new AnnotationRect(100, 600, 200, FreeTextInput.MinHeightPt), $"too short stops at the minimum, got {short1}");
    }

    private static void PageSpace()
    {
        Check(FreeTextInput.UnrotatedPageSize(new PageDimensions(595, 842, PageRotation.None)) == (595, 842), "upright page keeps its size");
        Check(FreeTextInput.UnrotatedPageSize(new PageDimensions(842, 595, PageRotation.Clockwise90)) == (595, 842), "a quarter turn swaps back to page space");
        Check(FreeTextInput.UnrotatedPageSize(new PageDimensions(595, 842, PageRotation.Clockwise180)) == (595, 842), "a half turn keeps the size");
        Check(FreeTextInput.UnrotatedPageSize(new PageDimensions(842, 595, PageRotation.Clockwise270)) == (595, 842), "three quarters swaps back");
    }

    private static void DrawPlan()
    {
        var layout = new FreeTextLayout(12, [
            new FreeTextLine("Canción, años", 2, 10.66),
            new FreeTextLine("¿qué? Ü € ñ", 2, 24.46),
        ], false);
        var box = new Annotation(5, 0, AnnotationKind.FreeText, new AnnotationRect(100, 600, 200, 50), null, [], "Canción, años ¿qué? Ü € ñ", layout);

        var plan = FreeTextDraw.Plan(box, 1)!;
        Check(plan.Lines.Select(line => line.Text).SequenceEqual(["Canción, años", "¿qué? Ü € ñ"]), "the overlay draws the core's lines verbatim, accents intact");
        Check(plan.FontSizePx == 12 && plan.WidthPx == 200 && plan.HeightPx == 50, "scale 1 draws in points");
        Check(Math.Abs(plan.Lines[0].Left - 2) < 1e-9, "x follows the core line");
        Check(Math.Abs(plan.Lines[0].Top - (10.66 - FreeTextDraw.AscentEm * 12)) < 1e-9, "top puts the baseline where the core says");

        var zoomed = FreeTextDraw.Plan(box, 2)!;
        Check(zoomed.FontSizePx == 24 && zoomed.WidthPx == 400 && zoomed.HeightPx == 100, "zoom 2x doubles the font and the box");
        Check(Math.Abs(zoomed.Lines[1].Left - 4) < 1e-9 && Math.Abs(zoomed.Lines[1].Top - (24.46 * 2 - FreeTextDraw.AscentEm * 24)) < 1e-9, "zoom 2x doubles every position");
        Check(zoomed.Lines.Select(line => line.Text).SequenceEqual(plan.Lines.Select(line => line.Text)), "zoom never rewraps");

        Check(FreeTextDraw.Plan(box with { Kind = AnnotationKind.TextNote }, 1) is null, "only a FreeText has a plan");
        Check(FreeTextDraw.Plan(box with { Layout = null }, 1) is null, "no core layout, nothing to draw (never a shell wrap)");
        Check(FreeTextDraw.Plan(box with { Rect = null }, 1) is null, "no rect, nothing to draw");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
