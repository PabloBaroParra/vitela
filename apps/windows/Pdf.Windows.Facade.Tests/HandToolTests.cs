using Pdf.Windows.Viewer;

/// <summary>
/// The hand tool's rules without a window, mirroring the GTK shell's
/// <c>app/pan.rs</c> and <c>app/pan/math.rs</c> tests.
/// </summary>
internal static class HandToolTests
{
    public static Task RunAsync()
    {
        Grabs();
        Panned();
        SpaceArmsAndRepeats();
        SpaceAwayOrWithoutPagesIsLeftAlone();
        TextFieldsKeepSpace();
        PrimaryWithoutSpaceReachesTheTools();
        SpacePrimaryDragMovesBothAxes();
        MiddleDragNeedsNoSpace();
        ReleasingSpaceMidDrag();
        ResetClearsEverything();
        return Task.CompletedTask;
    }

    private static void Grabs()
    {
        Check(HandTool.Grabs(PanButton.Middle, false) && HandTool.Grabs(PanButton.Middle, true), "middle grabs with or without Space");
        Check(!HandTool.Grabs(PanButton.Primary, false) && HandTool.Grabs(PanButton.Primary, true), "primary grabs only with Space");
        Check(!HandTool.Grabs(PanButton.Other, true) && !HandTool.Grabs(PanButton.Other, false), "other buttons never grab");
    }

    private static void Panned()
    {
        Check(HandTool.Panned(400, 30, 800) == 370 && HandTool.Panned(400, -30, 800) == 430
            && HandTool.Panned(400, 0, 800) == 400, "content follows the pointer");
        Check(HandTool.Panned(100, 500, 800) == 0 && HandTool.Panned(700, -500, 800) == 800, "clamped to both ends");
        Check(HandTool.Panned(0, -80, 0) == 0 && HandTool.Panned(0, 80, -50) == 0, "nothing scrollable cannot move");
        foreach (var offset in new[] { double.NaN, double.PositiveInfinity, double.NegativeInfinity })
            Check(HandTool.Panned(400, offset, 800) == 400, $"non-finite offset {offset} keeps the start");
        Check(HandTool.Panned(400, 10, double.NaN) == 0, "a non-finite extent is treated as unscrollable");
    }

    private static void SpaceArmsAndRepeats()
    {
        var hand = new HandTool();
        Check(hand.PressSpace(false, true, true) && hand.Cursor == PanCursor.Grab, "Space over the document arms the hand");
        Check(hand.PressSpace(false, false, true) && hand.Cursor == PanCursor.Grab, "an autorepeat keeps it armed even off the document");
        Check(hand.ReleaseSpace() && hand.Cursor == PanCursor.None, "one release disarms after any repeat");
        Check(!hand.Begin(PanButton.Primary, true, 0, 0), "a disarmed primary press stays with the tools");
        Check(!hand.ReleaseSpace(), "a release the hand never took is not swallowed");
    }

    private static void SpaceAwayOrWithoutPagesIsLeftAlone()
    {
        var hand = new HandTool();
        Check(!hand.PressSpace(false, false, true) && hand.Cursor == PanCursor.None, "Space away from the document");
        Check(!hand.PressSpace(false, true, false) && hand.Cursor == PanCursor.None, "Space with nothing to pan");
        Check(!hand.Begin(PanButton.Middle, false, 0, 0), "middle drag with nothing to pan");
        Check(!hand.Dragging, "no drag without pages");
    }

    private static void TextFieldsKeepSpace()
    {
        var hand = new HandTool();
        Check(!hand.PressSpace(true, true, true) && !hand.SpaceHeld, "a focused text field keeps Space");
        Check(hand.PressSpace(false, true, true), "armed");
        Check(!hand.PressSpace(true, true, true) && !hand.SpaceHeld && hand.Cursor == PanCursor.None,
            "focus moving into a text field hands the next repeat back");
        Check(!hand.Begin(PanButton.Primary, true, 0, 0), "and disarms the primary button");
    }

    private static void PrimaryWithoutSpaceReachesTheTools()
    {
        var hand = new HandTool();
        Check(!hand.Begin(PanButton.Primary, true, 400, 400), "primary press without Space is not claimed");
        Check(hand.Update(50, 50, 800, 1700) is null && hand.Cursor == PanCursor.None, "and moves nothing");
    }

    private static void SpacePrimaryDragMovesBothAxes()
    {
        var hand = new HandTool();
        hand.PressSpace(false, true, true);
        Check(hand.Begin(PanButton.Primary, true, 400, 400) && hand.Cursor == PanCursor.Grabbing, "Space + primary claims");
        Check(hand.Update(30, -40, 800, 1700) == (370, 440), "both axes follow the pointer");
        Check(hand.Update(60, -80, 800, 1700) == (340, 480), "offsets are from the press, not the previous move");
        Check(hand.Update(5000, -5000, 800, 1700) == (0, 1700), "clamped to the scroll extents");
        Check(hand.Begin(PanButton.Primary, true, 0, 0) && hand.Update(0, 0, 800, 1700) == (400, 400),
            "the same press seen twice keeps its origin");
        hand.End();
        Check(hand.Cursor == PanCursor.Grab, "open hand again after the button comes up");
        hand.ReleaseSpace();
        Check(hand.Cursor == PanCursor.None, "normal cursor after Space");
    }

    private static void MiddleDragNeedsNoSpace()
    {
        var hand = new HandTool();
        Check(hand.Begin(PanButton.Middle, true, 400, 400) && hand.Cursor == PanCursor.Grabbing, "middle claims without Space");
        Check(hand.Update(-25, 25, 800, 1700) == (425, 375), "middle pans");
        hand.End();
        Check(hand.Cursor == PanCursor.None, "normal cursor after a middle drag");
        Check(!hand.Begin(PanButton.Other, true, 0, 0), "the secondary button stays with the tools");

        hand.PressSpace(false, true, true);
        Check(hand.Begin(PanButton.Middle, true, 400, 400), "middle with Space");
        hand.ReleaseSpace();
        Check(hand.Update(10, 10, 800, 1700) == (390, 390) && hand.Cursor == PanCursor.Grabbing,
            "releasing Space mid middle-drag keeps panning");
        hand.End();
    }

    private static void ReleasingSpaceMidDrag()
    {
        var hand = new HandTool();
        hand.PressSpace(false, true, true);
        hand.Begin(PanButton.Primary, true, 400, 400);
        Check(hand.Update(10, 10, 800, 1700) == (390, 390), "pans while Space is held");
        hand.ReleaseSpace();
        Check(hand.Update(100, 100, 800, 1700) is null && hand.Cursor == PanCursor.None, "stops when Space comes up");
        Check(hand.Dragging && hand.Begin(PanButton.Primary, true, 0, 0),
            "but the press stays claimed until the button comes up");
        hand.End();
        Check(!hand.Dragging && !hand.Begin(PanButton.Primary, true, 0, 0), "released; the next primary press is the tools'");
    }

    private static void ResetClearsEverything()
    {
        var hand = new HandTool();
        hand.PressSpace(false, true, true);
        hand.Begin(PanButton.Primary, true, 400, 400);
        hand.Reset();
        Check(hand.Cursor == PanCursor.None && !hand.Dragging && !hand.SpaceHeld, "reset drops Space and the drag");
        Check(hand.Update(50, 50, 800, 1700) is null, "nothing moves after a reset");
        Check(!hand.Begin(PanButton.Primary, true, 0, 0), "Space is no longer held");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
