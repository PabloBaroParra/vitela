using Pdf.Windows.Viewer;

/// <summary>
/// The document tab strip's rules without a window: the list of tabs, which
/// tab takes over when one closes, how a path finds the tab holding it, where
/// an open lands, and which files of a drop open. Ported from the GTK shell's
/// <c>tabs/list.rs</c> tests, plus the Windows-only routing helpers.
/// </summary>
internal static class DocumentTabsTests
{
    public static Task RunAsync()
    {
        ANewTabGoesLastAndIsNotActivated();
        IdsAreNeverReused();
        ActivatingAnUnknownTabChangesNothing();
        ClosingAnInactiveTabKeepsTheActiveOne();
        ClosingTheActiveTabHandsOverToItsRightNeighbour();
        ClosingTheLastActiveTabHandsOverToItsLeftNeighbour();
        ClosingTheOnlyTabLeavesTheStripEmpty();
        ClosingATabThatIsGoneAnswersNothing();
        APathFindsTheTabThatHoldsIt();
        ATabWithoutAPathIsNeverFoundByOne();
        AMovedDocumentIsFoundAtItsNewPath();
        OfTwoTabsHoldingOnePathTheActiveOneIsFound();
        PathsCompareWithoutCaseOrRelativeSegments();
        AnotherTabHoldingAPathIsReportedButTheTabItselfIsNot();
        DeactivatingParksTheActiveTabWithoutClosingIt();
        CyclingWrapsAroundTheStripBothWays();
        CyclingWithOneTabOrNoneStaysPut();
        OpensRouteByPlacementAndHolder();
        ADropOpensEveryPdfAndOtherwiseActsOnItsFirstFile();
        ALaterFileLandsInFrontWhileNoTabIsActive();
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static (DocumentTabList List, TabId A, TabId B, TabId C) Three()
    {
        var list = new DocumentTabList();
        var a = list.Add(@"C:\docs\a.pdf");
        var b = list.Add(@"C:\docs\b.pdf");
        var c = list.Add(null);
        return (list, a, b, c);
    }

    private static void ANewTabGoesLastAndIsNotActivated()
    {
        var (list, a, b, c) = Three();
        Check(list.Ids.SequenceEqual([a, b, c]), "tabs are appended in strip order");
        Check(list.Active is null, "adding a tab never activates it");
        Check(list.Count == 3 && !list.IsEmpty, "the list counts its tabs");
    }

    private static void IdsAreNeverReused()
    {
        var (list, a, _, _) = Three();
        list.Close(a);
        var d = list.Add(null);
        Check(d != a && !list.Contains(a), "a closed tab's id names nothing, not a newer tab");
    }

    private static void ActivatingAnUnknownTabChangesNothing()
    {
        var (list, a, _, _) = Three();
        list.Activate(a);
        var gone = list.Add(null);
        list.Close(gone);
        Check(!list.Activate(gone) && list.Active == a, "an unknown id is refused and the active tab stays");
    }

    private static void ClosingAnInactiveTabKeepsTheActiveOne()
    {
        var (list, a, b, _) = Three();
        list.Activate(a);
        var after = list.Close(b);
        Check(after is { Kind: AfterCloseKind.Unchanged } && list.Active == a, "closing a background tab keeps the active one");
    }

    private static void ClosingTheActiveTabHandsOverToItsRightNeighbour()
    {
        var (list, a, b, c) = Three();
        list.Activate(b);
        var after = list.Close(b);
        Check(after is { Kind: AfterCloseKind.Activate } && after.Value.Next == c, "the right-hand neighbour takes over");
        Check(list.Active is null, "the caller activates the next tab once its document is installed");
        Check(list.Ids.SequenceEqual([a, c]), "the closed tab leaves the strip");
    }

    private static void ClosingTheLastActiveTabHandsOverToItsLeftNeighbour()
    {
        var (list, _, b, c) = Three();
        list.Activate(c);
        var after = list.Close(c);
        Check(after is { Kind: AfterCloseKind.Activate } && after.Value.Next == b, "the left-hand neighbour takes over when the last tab closes");
    }

    private static void ClosingTheOnlyTabLeavesTheStripEmpty()
    {
        var list = new DocumentTabList();
        var only = list.Add(null);
        list.Activate(only);
        var after = list.Close(only);
        Check(after is { Kind: AfterCloseKind.Empty } && list.IsEmpty && list.Active is null, "closing the only tab empties the strip");
    }

    private static void ClosingATabThatIsGoneAnswersNothing()
    {
        var (list, a, _, _) = Three();
        list.Close(a);
        Check(list.Close(a) is null, "a second close of the same tab is refused");
    }

    private static void APathFindsTheTabThatHoldsIt()
    {
        var (list, _, b, _) = Three();
        Check(list.FindByPath(@"C:\docs\b.pdf") == b, "a path finds its tab");
        Check(list.FindByPath(@"C:\docs\missing.pdf") is null, "a path no tab holds finds nothing");
    }

    private static void ATabWithoutAPathIsNeverFoundByOne()
    {
        var (list, _, _, c) = Three();
        Check(list.PathOf(c) is null && list.FindByPath("") is null, "a pathless tab is never found");
    }

    private static void AMovedDocumentIsFoundAtItsNewPath()
    {
        var (list, a, _, _) = Three();
        list.SetPath(a, @"C:\docs\renamed.pdf");
        Check(list.FindByPath(@"C:\docs\renamed.pdf") == a && list.FindByPath(@"C:\docs\a.pdf") is null, "Save As moves the tab's path");
    }

    private static void OfTwoTabsHoldingOnePathTheActiveOneIsFound()
    {
        var list = new DocumentTabList();
        var first = list.Add(@"C:\docs\same.pdf");
        var second = list.Add(@"C:\docs\same.pdf");
        Check(list.FindByPath(@"C:\docs\same.pdf") == first, "without an active holder the first in strip order wins");
        list.Activate(second);
        Check(list.FindByPath(@"C:\docs\same.pdf") == second, "the active holder wins");
        Check(list.Holders(@"C:\docs\same.pdf").SequenceEqual([first, second]), "every holder is listed in strip order");
    }

    private static void PathsCompareWithoutCaseOrRelativeSegments()
    {
        var list = new DocumentTabList();
        var tab = list.Add(@"C:\Docs\Report.PDF");
        Check(list.FindByPath(@"c:\docs\sub\..\report.pdf") == tab, "Windows paths compare case-insensitively after normalisation");
        Check(TabPaths.Normalize(null) is null && TabPaths.Normalize("   ") is null, "no path normalises to none");
        Check(TabPaths.Normalize("relative.pdf") is null, "a path that is not fully qualified names no file a tab can hold");
    }

    private static void AnotherTabHoldingAPathIsReportedButTheTabItselfIsNot()
    {
        var (list, a, b, _) = Three();
        Check(list.HeldByAnotherTab(@"C:\docs\b.pdf", a), "Save As onto a path another tab holds is refused");
        Check(!list.HeldByAnotherTab(@"C:\docs\a.pdf", a), "Save As onto the tab's own path is allowed");
        Check(!list.HeldByAnotherTab(@"C:\docs\new.pdf", a), "a free path is allowed");
        Check(list.HeldByAnotherTab(@"C:\docs\b.pdf", null), "with no tab of its own, any holder counts");
        _ = b;
    }

    private static void DeactivatingParksTheActiveTabWithoutClosingIt()
    {
        var (list, a, _, _) = Three();
        list.Activate(a);
        list.Deactivate();
        Check(list.Active is null && list.Contains(a), "a parked tab stays in the strip");
    }

    private static void CyclingWrapsAroundTheStripBothWays()
    {
        var (list, a, b, c) = Three();
        list.Activate(c);
        Check(list.Cycle(+1) == a, "Ctrl+Tab from the last tab wraps to the first");
        list.Activate(a);
        Check(list.Cycle(-1) == c, "Ctrl+Shift+Tab from the first tab wraps to the last");
        Check(list.Cycle(+1) == b, "Ctrl+Tab moves one tab right");
    }

    private static void CyclingWithOneTabOrNoneStaysPut()
    {
        var empty = new DocumentTabList();
        Check(empty.Cycle(+1) is null, "no tabs means nowhere to go");
        var list = new DocumentTabList();
        var only = list.Add(null);
        list.Activate(only);
        Check(list.Cycle(+1) is null && list.Cycle(-1) is null, "a single tab has nowhere to switch to");
    }

    private static void OpensRouteByPlacementAndHolder()
    {
        var (list, a, b, _) = Three();
        list.Activate(a);
        Check(TabOpenRouting.Route(front: true, frontAllowed: true, holder: null, active: a) == TabOpenRoute.OpenInFront,
            "the first file of an action opens in front");
        Check(TabOpenRouting.Route(front: true, frontAllowed: false, holder: null, active: a) == TabOpenRoute.OpenInBackground,
            "a front open the active tab cannot give way to lands in the background");
        Check(TabOpenRouting.Route(front: false, frontAllowed: true, holder: null, active: a) == TabOpenRoute.OpenInBackground,
            "the rest of an action's files land in background tabs");
        Check(TabOpenRouting.Route(front: true, frontAllowed: true, holder: b, active: a) == TabOpenRoute.SwitchTo,
            "reopening a file held by a background tab switches to it");
        Check(TabOpenRouting.Route(front: true, frontAllowed: true, holder: a, active: a) == TabOpenRoute.Reload,
            "reopening the file on screen reads it again");
        Check(TabOpenRouting.Route(front: false, frontAllowed: true, holder: b, active: a) == TabOpenRoute.Skip,
            "a background open of a held file changes nothing");
        Check(TabOpenRouting.Route(front: true, frontAllowed: false, holder: a, active: a) == TabOpenRoute.Skip,
            "a reload the active tab cannot give way to is not attempted");
        Check(TabOpenRouting.Route(front: true, frontAllowed: true, holder: null, active: null) == TabOpenRoute.OpenInFront,
            "with nothing open the first file lands in front");
    }

    private static void ALaterFileLandsInFrontWhileNoTabIsActive()
    {
        Check(TabOpenRouting.IsFront(0, anyTabActive: true) && TabOpenRouting.IsFront(0, anyTabActive: false),
            "the first file of an action always asks for the front");
        Check(!TabOpenRouting.IsFront(1, anyTabActive: true), "later files land behind the tab on screen");
        Check(TabOpenRouting.IsFront(1, anyTabActive: false) && TabOpenRouting.IsFront(4, anyTabActive: false),
            "after a failed first file with nothing on screen, the next file lands in front");
    }

    private static void ADropOpensEveryPdfAndOtherwiseActsOnItsFirstFile()
    {
        var plan = FileDropRouting.Plan<string>([("notes.txt", DroppedFileKind.Unsupported), ("a.pdf", DroppedFileKind.Document),
            ("logo.png", DroppedFileKind.ImageStamp), ("b.pdf", DroppedFileKind.Document)]);
        Check(plan.Documents.SequenceEqual(["a.pdf", "b.pdf"]) && plan.Single is null, "every dropped PDF opens, in drop order");
        plan = FileDropRouting.Plan<string>([("logo.png", DroppedFileKind.ImageStamp), ("notes.txt", DroppedFileKind.Unsupported)]);
        Check(plan.Documents.Count == 0 && plan.Single == "logo.png", "without a PDF the first file decides, as before");
        plan = FileDropRouting.Plan<string>([]);
        Check(plan.Documents.Count == 0 && plan.Single is null, "an empty drop plans nothing");
    }
}
