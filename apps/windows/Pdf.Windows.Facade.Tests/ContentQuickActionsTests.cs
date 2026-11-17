using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// The floating quick-action bar beside an Edit content selection (Linux PR
/// #567): where it goes on the canvas, when it stands down, and which of its
/// buttons a selection offers — each under the gate its existing door uses.
/// </summary>
internal static class ContentQuickActionsTests
{
    private const double BarWidth = 200, BarHeight = 30, CanvasWidth = 800, CanvasHeight = 600;
    private const double Gap = ContentQuickActions.GapPx;

    public static Task RunAsync()
    {
        TheBarSitsRightOfTheTopRightCorner();
        ItFlipsLeftThenAboveThenBelow();
        ASelectionOffAnEdgeKeepsItsBarOnTheCanvas();
        ASelectionScrolledOutOfViewHasNoBar();
        TheBarStandsDownWhileDraggingEditingOrDisarmed();
        EachSelectionOffersItsOwnActions();
        return Task.CompletedTask;
    }

    private static (double X, double Y)? Origin(PlacedRect anchor, double canvasWidth = CanvasWidth) =>
        ContentQuickActions.BarOrigin(anchor, BarWidth, BarHeight, canvasWidth, CanvasHeight);

    private static void TheBarSitsRightOfTheTopRightCorner() =>
        Check(Origin(new PlacedRect(100, 100, 150, 40)) == (250 + Gap, 100), "right of the selection, top edges level");

    private static void ItFlipsLeftThenAboveThenBelow()
    {
        Check(Origin(new PlacedRect(500, 100, 250, 40)) == (500 - Gap - 200, 100), "left when the right has no room");
        Check(Origin(new PlacedRect(100, 300, 650, 40)) == (750 - 200, 300 - Gap - 30), "above when neither side has room");
        Check(Origin(new PlacedRect(100, 10, 650, 40)) == (550, 50 + Gap), "below when there is no room above either");
    }

    private static void ASelectionOffAnEdgeKeepsItsBarOnTheCanvas()
    {
        Check(Origin(new PlacedRect(100, 590, 100, 40)) == (206, 570), "a selection running off the bottom pulls its bar back up");
        Check(Origin(new PlacedRect(10, 10, 20, 20), canvasWidth: 150) == (0, 36), "a canvas narrower than the bar pins it left, below");
    }

    private static void ASelectionScrolledOutOfViewHasNoBar()
    {
        Check(Origin(new PlacedRect(100, -80, 50, 40)) is null, "above the canvas");
        Check(Origin(new PlacedRect(100, 700, 50, 40)) is null, "below the canvas");
        Check(Origin(new PlacedRect(-300, 10, 50, 40)) is null, "left of the canvas");
    }

    private static void TheBarStandsDownWhileDraggingEditingOrDisarmed()
    {
        static bool May(bool modeOn = true, bool insertArmed = false, bool dragging = false, bool editorOpen = false,
            bool organizing = false, bool dialogOpen = false) =>
            ContentQuickActions.MayShow(modeOn, insertArmed, dragging, editorOpen, organizing, dialogOpen);
        Check(May(), "shown with the mode on and nothing in the way");
        Check(!May(modeOn: false), "hidden with the mode off");
        Check(!May(insertArmed: true), "hidden with an insert kind armed");
        Check(!May(dragging: true), "hidden while a press is dragged");
        Check(!May(editorOpen: true), "hidden under an inline editor");
        Check(!May(organizing: true), "hidden while organizing");
        Check(!May(dialogOpen: true), "hidden behind a dialog");
    }

    private static void EachSelectionOffersItsOwnActions()
    {
        static QuickActionsOffer Offer(QuickActionsSelection selection, bool idle = true, bool editable = true,
            bool replace = true, bool imageDelete = true, bool shapeDelete = true, bool editInFlight = false) =>
            ContentQuickActions.Offer(selection, idle, editable, replace, imageDelete, shapeDelete, editInFlight);

        var block = Offer(QuickActionsSelection.TextBlock);
        Check(block == new QuickActionsOffer(Retype: true, Replace: null, Cut: true, Copy: true, Delete: true), "a block retypes, cuts, copies and deletes");
        Check(Offer(QuickActionsSelection.TextBlock, editable: false)
            == new QuickActionsOffer(Retype: false, Replace: null, Cut: false, Copy: true, Delete: false), "a locked document keeps copy and loses the rest");

        Check(Offer(QuickActionsSelection.Image) == new QuickActionsOffer(null, Replace: true, Cut: true, Copy: true, Delete: true), "an image replaces");
        Check(Offer(QuickActionsSelection.Image, replace: false, imageDelete: true).Replace == false, "a pending edit keeps Replace shown, disabled");
        Check(!Offer(QuickActionsSelection.Image, imageDelete: false).Delete, "an image deletes when its card's Delete would");

        Check(Offer(QuickActionsSelection.Shape) == new QuickActionsOffer(null, null, Cut: true, Copy: true, Delete: true), "a shape cuts, copies, deletes");
        Check(!Offer(QuickActionsSelection.Shape, shapeDelete: false).Delete, "a shape deletes when its card's Delete would");

        Check(Offer(QuickActionsSelection.Several) == new QuickActionsOffer(null, null, Cut: true, Copy: true, Delete: true), "several items cut, copy, delete");

        var busy = Offer(QuickActionsSelection.TextBlock, idle: false);
        Check(busy == new QuickActionsOffer(Retype: false, Replace: null, Cut: false, Copy: false, Delete: false), "nothing may be pressed while busy");

        // An image or shape edit in flight (the Replace picker open, a drag
        // being recorded) keeps copy and refuses every change.
        Check(Offer(QuickActionsSelection.TextBlock, editInFlight: true)
            == new QuickActionsOffer(Retype: false, Replace: null, Cut: false, Copy: true, Delete: false), "a block waits for the edit in flight");
        Check(Offer(QuickActionsSelection.Image, editInFlight: true)
            == new QuickActionsOffer(null, Replace: false, Cut: false, Copy: true, Delete: false), "an image waits for the edit in flight");
        Check(Offer(QuickActionsSelection.Shape, editInFlight: true)
            == new QuickActionsOffer(null, null, Cut: false, Copy: true, Delete: false), "a shape waits for the edit in flight");
        Check(Offer(QuickActionsSelection.Several, editInFlight: true)
            == new QuickActionsOffer(null, null, Cut: false, Copy: true, Delete: false), "several items wait for the edit in flight");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
