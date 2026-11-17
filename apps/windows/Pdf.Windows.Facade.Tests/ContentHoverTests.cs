using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// The Edit content hover (Linux PR #567): the outline two screen pixels
/// outside the item under the pointer, the border band four pixels either
/// side of it, and the checks that keep a stale hover from grabbing the wrong
/// item after the page was parsed again.
/// </summary>
internal static class ContentHoverTests
{
    private static readonly ContentTextRun Cliente = Run(0, new PdfCoreRect(310, 650, 40, 12), "Cliente");
    private static readonly ContentImage Logo = new(new PdfCoreContentImage(5, 0, new PdfCoreRect(100, 620, 60, 40), "Im1"), "s", 1);
    private static readonly ContentPath Frame = new(new PdfCoreContentPath(3, 0, new PdfCoreRect(300, 600, 200, 80), PdfCorePathPaint.Stroke), "s", 1);

    public static Task RunAsync()
    {
        OutlineSitsTwoScreenPixelsOutAtAnyZoom();
        EdgeDistanceIsMeasuredFromInsideAndOutside();
        ClassifiesTheBandInsideAndClear();
        HandlesOutrankTheBandOnlyForTheSelectedItem();
        ResolvesWhatTheHitTestFinds();
        HoldsTheHoveredItemOnTheOuterBand();
        RefindsTheItemByItsOuterBandOnlyWhenAsked();
        AStaleHoverIsNotCurrent();
        GatesAndCursor();
        TheBorderClaimsOnlyWhatIsUnderThePressOrNothing();
        AHeldGroupMemberHasNoHoverOfItsOwn();
        return Task.CompletedTask;
    }

    private static ContentTextRun Run(ulong id, PdfCoreRect box, string text) =>
        new(new PdfCoreContentTextRun(id, 0, box, "F1", PdfCoreFontKind.Standard14, text), null);

    private static ContentTextBlock Block(params ContentTextRun[] runs) =>
        new(runs, runs[0].Bounds, string.Join(" ", runs.Select(run => run.Text)));

    private static PageContent Page(IReadOnlyList<ContentTextRun> runs, IReadOnlyList<ContentImage> images, IReadOnlyList<ContentPath> paths) =>
        new(0, runs, images, paths) { TextBlocks = runs.Select(run => Block(run)).ToArray() };

    private static void OutlineSitsTwoScreenPixelsOutAtAnyZoom()
    {
        var box = new AnnotationRect(100, 200, 50, 10);
        Check(ContentHover.OutlineRect(box, 1) == new AnnotationRect(98, 198, 54, 14), "2 points out at 100%");
        Check(ContentHover.OutlineRect(box, 2) == new AnnotationRect(99, 199, 52, 12), "1 point out at 200%: still 2 screen pixels");
        Check(ContentHover.OutlineRect(box, 0) == new AnnotationRect(98, 198, 54, 14), "an unlaid page falls back to one pixel per point");
        Check(ContentHover.OutlineRect(box, double.NaN) == new AnnotationRect(98, 198, 54, 14), "a NaN scale falls back too");
    }

    private static void EdgeDistanceIsMeasuredFromInsideAndOutside()
    {
        var box = new AnnotationRect(0, 0, 10, 10);
        Check(ContentHover.EdgeDistance(box, new(5, 5)) == 5, "the centre is half the side from every edge");
        Check(ContentHover.EdgeDistance(box, new(1, 5)) == 1, "inside, the nearest edge counts");
        Check(ContentHover.EdgeDistance(box, new(13, 5)) == 3, "outside a side, the straight distance");
        Check(ContentHover.EdgeDistance(box, new(13, 14)) == 5, "outside a corner, the diagonal");
    }

    private static void ClassifiesTheBandInsideAndClear()
    {
        var box = new AnnotationRect(100, 100, 100, 100);
        // At 100% the outline is at x=98 and the band spans 94..102.
        Check(ContentHover.Classify(box, new(98, 150), 1) == ContentHoverZone.Border, "on the outline");
        Check(ContentHover.Classify(box, new(94, 150), 1) == ContentHoverZone.Border, "the band's outer edge");
        Check(ContentHover.Classify(box, new(102, 150), 1) == ContentHoverZone.Border, "the band's inner edge");
        Check(ContentHover.Classify(box, new(93.5, 150), 1) is null, "past the band is clear");
        Check(ContentHover.Classify(box, new(110, 150), 1) == ContentHoverZone.Inside, "inside, clear of the band");
        // At 400% the band shrinks to 1pt either side of an outline 0.5pt out.
        Check(ContentHover.Classify(box, new(98, 150), 4) is null, "4 screen px out at 400% is past the band");
        Check(ContentHover.Classify(box, new(99, 150), 4) == ContentHoverZone.Border, "the band at 400%");
        Check(ContentHover.Classify(box, new(101, 150), 4) == ContentHoverZone.Inside, "inside at 400%");
    }

    private static void HandlesOutrankTheBandOnlyForTheSelectedItem()
    {
        var box = new AnnotationRect(100, 100, 100, 100);
        Check(ContentHover.ZoneOf(box, new(101, 101), 1, 8, carriesHandles: true) == ContentHoverZone.Handle, "a selected item's corner is its handle");
        Check(ContentHover.ZoneOf(box, new(101, 101), 1, 8, carriesHandles: false) == ContentHoverZone.Border, "an unselected item's corner is border");
        Check(ContentHover.ZoneOf(box, new(150, 101), 1, 8, carriesHandles: true) == ContentHoverZone.Border, "away from the corners the band still counts");
    }

    private static void ResolvesWhatTheHitTestFinds()
    {
        var logo = ContentHoverMember.Of(Logo);
        ContentHoverMember? HitLogo(AnnotationPoint point) =>
            point.X >= 100 && point.X <= 160 && point.Y >= 620 && point.Y <= 660 ? logo : null;
        var inside = ContentHover.Resolve(0, HitLogo, null, new(130, 640), 1, 8, _ => false);
        Check(inside == new ContentHoverState(0, logo, ContentHoverZone.Inside), "the middle of the logo is inside it");
        var edge = ContentHover.Resolve(0, HitLogo, null, new(101, 640), 1, 8, _ => false);
        Check(edge?.Zone == ContentHoverZone.Border, "just inside its edge is its border");
        Check(ContentHover.Resolve(0, HitLogo, null, new(300, 300), 1, 8, _ => false) is null, "nothing hit, nothing held");

        // A path hit within reach of its edge but past the band is its border.
        var frame = ContentHoverMember.Of(Frame);
        var far = ContentHover.Resolve(0, _ => frame, null, new(292, 640), 1, 8, _ => false);
        Check(far?.Zone == ContentHoverZone.Border, "a hit the band misses still reads as border");
    }

    private static void HoldsTheHoveredItemOnTheOuterBand()
    {
        var logo = ContentHoverMember.Of(Logo);
        var current = new ContentHoverState(0, logo, ContentHoverZone.Border);
        // x=96 lies 4pt left of the logo: off its box, on the band's outer half.
        Check(ContentHover.Resolve(0, _ => null, current, new(96, 640), 1, 8, _ => false)?.Member == logo, "the outer band keeps the hover");
        Check(ContentHover.Resolve(0, _ => null, current, new(90, 640), 1, 8, _ => false) is null, "past the band lets go");
        Check(ContentHover.Resolve(1, _ => null, current, new(96, 640), 1, 8, _ => false) is null, "another page's hover is not held");
    }

    private static void RefindsTheItemByItsOuterBandOnlyWhenAsked()
    {
        var logo = ContentHoverMember.Of(Logo);
        ContentHoverMember? HitLogo(AnnotationPoint point) =>
            point.X >= 100 && point.X <= 160 && point.Y >= 620 && point.Y <= 660 ? logo : null;
        Check(ContentHover.Resolve(0, HitLogo, null, new(96, 640), 1, 8, _ => false) is null, "motion alone does not refind the outer band");
        var found = ContentHover.Resolve(0, HitLogo, null, new(96, 640), 1, 8, _ => false, refind: true);
        Check(found == new ContentHoverState(0, logo, ContentHoverZone.Border), "a restore finds the item by its outer band");
        Check(ContentHover.Resolve(0, HitLogo, null, new(80, 640), 1, 8, _ => false, refind: true) is null, "but not from past the band");
    }

    private static void AStaleHoverIsNotCurrent()
    {
        var content = Page([Cliente], [Logo], [Frame]);
        var block = content.TextBlocks.Single();
        Check(ContentHover.StillCurrent(content, ContentHoverMember.Of(block, block.Bounds))?.Block == block, "a block still parsed is current");
        Check(ContentHover.StillCurrent(content, ContentHoverMember.Of(Logo))?.Image == Logo, "the logo is current");
        Check(ContentHover.StillCurrent(content, ContentHoverMember.Of(Frame))?.Path == Frame, "the frame is current");

        // Re-parsed after a move: same ids, other boxes.
        var moved = Page(
            [Run(0, new PdfCoreRect(310, 450, 40, 12), "Cliente")],
            [new ContentImage(new PdfCoreContentImage(5, 0, new PdfCoreRect(100, 420, 60, 40), "Im1"), "s", 2)],
            [new ContentPath(new PdfCoreContentPath(3, 0, new PdfCoreRect(300, 400, 200, 80), PdfCorePathPaint.Stroke), "s", 2)]);
        Check(ContentHover.StillCurrent(moved, ContentHoverMember.Of(block, block.Bounds)) is null, "a moved block is stale");
        Check(ContentHover.StillCurrent(moved, ContentHoverMember.Of(Logo)) is null, "a moved image is stale");
        Check(ContentHover.StillCurrent(moved, ContentHoverMember.Of(Frame)) is null, "a moved path is stale");

        // Ids are positional: the same id at the same box on a fresh parse is
        // the same item, and the member is re-picked from the parse.
        var reparsed = Page([Run(0, new PdfCoreRect(310, 650, 40, 12), "Cliente")], [], []);
        var again = ContentHover.StillCurrent(reparsed, ContentHoverMember.Of(block, block.Bounds));
        Check(again?.Block == reparsed.TextBlocks.Single(), "the current block comes from the page as parsed now");
        Check(ContentHover.StillCurrent(reparsed, ContentHoverMember.Of(Logo)) is null, "a deleted image is stale");
    }

    private static void GatesAndCursor()
    {
        Check(ContentHover.MayHover(modeOn: true, insertArmed: false, editorOpen: false, panning: false), "the armed mode hovers");
        Check(!ContentHover.MayHover(modeOn: false, insertArmed: false, editorOpen: false, panning: false), "not with the mode off");
        Check(!ContentHover.MayHover(modeOn: true, insertArmed: true, editorOpen: false, panning: false), "not with an insert kind armed");
        Check(!ContentHover.MayHover(modeOn: true, insertArmed: false, editorOpen: true, panning: false), "not with an editor open");
        Check(!ContentHover.MayHover(modeOn: true, insertArmed: false, editorOpen: false, panning: true), "not while panning");
        Check(ContentHover.ShowsMoveCursor(ContentHoverZone.Border), "the border shows the move cursor");
        Check(!ContentHover.ShowsMoveCursor(ContentHoverZone.Inside), "inside keeps the arrow");
        Check(!ContentHover.ShowsMoveCursor(ContentHoverZone.Handle), "a handle keeps the arrow");
        Check(!ContentHover.ShowsMoveCursor(null), "nothing hovered keeps the arrow");
    }

    private static void TheBorderClaimsOnlyWhatIsUnderThePressOrNothing()
    {
        var logo = ContentHoverMember.Of(Logo);
        var frame = ContentHoverMember.Of(Frame);
        Check(ContentHover.BorderMayClaim(hit: null, logo), "the outer band, where nothing is hit, is the border's");
        Check(ContentHover.BorderMayClaim(hit: logo, logo), "the inner band of the item itself is the border's");
        Check(!ContentHover.BorderMayClaim(hit: frame, logo), "another item under the press keeps it");
        var otherImage = ContentHoverMember.Of(new ContentImage(new PdfCoreContentImage(6, 0, new PdfCoreRect(110, 630, 60, 40), "Im2"), "s", 1));
        Check(!ContentHover.BorderMayClaim(hit: otherImage, logo), "another image under the press keeps it");
    }

    private static void AHeldGroupMemberHasNoHoverOfItsOwn()
    {
        var content = Page([Cliente], [Logo], [Frame]);
        var block = content.TextBlocks.Single();
        var held = ContentSelection.Empty(0).Toggle(Logo).Toggle(block);
        Check(ContentHover.InHeldGroup(held, 0, ContentHoverMember.Of(Logo)), "a held image is in the group");
        Check(ContentHover.InHeldGroup(held, 0, ContentHoverMember.Of(block, block.Bounds)), "a held block is in the group");
        Check(!ContentHover.InHeldGroup(held, 0, ContentHoverMember.Of(Frame)), "an item left out is not");
        Check(!ContentHover.InHeldGroup(held, 1, ContentHoverMember.Of(Logo)), "a group held on another page is not");
        Check(!ContentHover.InHeldGroup(null, 0, ContentHoverMember.Of(Logo)), "with nothing held, nothing is");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}
