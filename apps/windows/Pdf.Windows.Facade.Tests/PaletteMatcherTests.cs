using Pdf.Windows.Viewer;

/// <summary>
/// The command palette's fuzzy matcher and session recents, ported case for
/// case from the GTK shell's <c>palette/matcher.rs</c> and <c>palette/recent.rs</c>.
/// </summary>
internal static class PaletteMatcherTests
{
    public static Task RunAsync()
    {
        NormalizeLowercasesAndStripsDiacritics();
        NormalizeKeepsUnfoldedCharacters();
        NoSubsequenceIsNoMatch();
        AccentAndCaseInsensitive();
        WordStartBeatsMidWord();
        WordStartAfterSeparators();
        ContiguousBeatsScattered();
        PrefixBeatsNonPrefix();
        BestAlignmentIsFoundNotJustTheFirst();
        LaterCharactersPreferAWordStart();
        AContiguousRunIsNotBrokenForAWordStart();
        AWholeWordRunBeatsAnyScatteredAlignment();
        AWordStartThatStrandsTheRestIsSkipped();
        WordStartAfterPunctuationAndAnyWhitespace();
        MultiTermQueryNeedsEveryTerm();
        EmptyQueryMatchesEverythingEqually();
        NonEmptyQueryNeverMatchesEmptyCandidate();
        RankDropsNonMatchesAndSortsByScore();
        RankTiesKeepCatalogOrder();
        RankEmptyQueryKeepsEverythingInCatalogOrder();
        RankUsesTheBestHaystack();
        NothingRecentKeepsTheGivenOrder();
        TheMostRecentComesFirstThenTheRestInOrder();
        RunningACommandAgainMovesItToTheFrontOnce();
        AnIdNoLongerInTheListIsIgnored();
        return Task.CompletedTask;
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void Equal<T>(T expected, T actual, string message)
    {
        if (!EqualityComparer<T>.Default.Equals(expected, actual))
            throw new InvalidOperationException($"{message}: expected {expected}, got {actual}");
    }

    private static int S(string query, string candidate) =>
        PaletteMatcher.Score(query, candidate) ?? throw new InvalidOperationException($"\"{query}\" should match \"{candidate}\"");

    private static void NormalizeLowercasesAndStripsDiacritics()
    {
        Equal("pagina", PaletteMatcher.Normalize("Página"), "Página");
        Equal("pagina", PaletteMatcher.Normalize("PÁGINA"), "PÁGINA");
        Equal("anadir", PaletteMatcher.Normalize("Añadir"), "Añadir");
        Equal("u", PaletteMatcher.Normalize("Ü"), "Ü");
        Equal("acao ca", PaletteMatcher.Normalize("Ação Çà"), "Ação Çà");
        Equal("oeuvre strasse", PaletteMatcher.Normalize("Œuvre Straße"), "Œuvre Straße");
    }

    private static void NormalizeKeepsUnfoldedCharacters()
    {
        Equal("pdf-2 / x_y", PaletteMatcher.Normalize("PDF-2 / x_y"), "separators");
        Equal("日本", PaletteMatcher.Normalize("日本"), "CJK");
    }

    private static void NoSubsequenceIsNoMatch()
    {
        Equal(null, PaletteMatcher.Score("xyz", "Rotate page"), "xyz");
        Equal(null, PaletteMatcher.Score("egap", "page"), "egap");
        Equal(null, PaletteMatcher.Score("pages", "page"), "pages");
    }

    private static void AccentAndCaseInsensitive()
    {
        Check(PaletteMatcher.Score("pagina", "Página") is not null, "pagina/Página");
        Check(PaletteMatcher.Score("pagina", "PÁGINA") is not null, "pagina/PÁGINA");
        Check(PaletteMatcher.Score("PÁGINA", "pagina") is not null, "PÁGINA/pagina");
        Equal(PaletteMatcher.Score("pagina", "pagina"), PaletteMatcher.Score("pagina", "Página"), "accent does not change the score");
    }

    private static void WordStartBeatsMidWord()
    {
        Check(S("p", "Rotate page") > S("p", "Stamp"), "p at a word start");
        Check(S("ex", "Open export") > S("ex", "Open text"), "ex at a word start");
    }

    private static void WordStartAfterSeparators()
    {
        var mid = S("b", "xab");
        foreach (var candidate in new[] { "xa b", "xa-b", "xa_b", "xa/b" })
            Check(S("b", candidate) > mid, candidate);
    }

    private static void ContiguousBeatsScattered()
    {
        Check(S("ota", "xotax") > S("ota", "xoxtxa"), "xotax");
        Check(S("ota", "rotate") > S("ota", "rooxtxa"), "rotate");
    }

    private static void PrefixBeatsNonPrefix() =>
        Check(S("rot", "Rotate page") > S("rot", "Page rotate"), "prefix");

    private static void BestAlignmentIsFoundNotJustTheFirst() =>
        Check(S("pa", "Stamp page") > S("pa", "Stamp xaxa"), "a later word start wins");

    private static void LaterCharactersPreferAWordStart()
    {
        Check(S("ep", "Export pages") > S("ep", "Export xages"), "ep");
        Equal((PaletteMatcher.Match + PaletteMatcher.WordStart + PaletteMatcher.Prefix) + (PaletteMatcher.Match + PaletteMatcher.WordStart),
            S("ep", "Export pages"), "ep score");
    }

    private static void AContiguousRunIsNotBrokenForAWordStart() =>
        Equal((PaletteMatcher.Match + PaletteMatcher.WordStart + PaletteMatcher.Prefix)
                + 2 * (PaletteMatcher.Match + PaletteMatcher.Contiguous) + PaletteMatcher.RunBonus(3),
            S("rot", "Rotate options"), "rot stays one run");

    private static void AWholeWordRunBeatsAnyScatteredAlignment()
    {
        Check(S("pdf", "Añadir PDF") > S("pdf", "pon de formulario"), "pdf vs scattered word starts");
        Check(S("pdf", "Añadir PDF") > S("pdf", "Campo de texto formulario"), "pdf vs a scattered prefix");
        Check(S("pag", "Rotate pages") > S("pag", "Pon a grid"), "word prefix");
    }

    private static void AWordStartThatStrandsTheRestIsSkipped() =>
        Check(PaletteMatcher.Score("epx", "Expx pq") is not null, "epx");

    private static void WordStartAfterPunctuationAndAnyWhitespace()
    {
        var mid = S("z", "xaz");
        foreach (var candidate in new[] { "xa(z", "xa)z", "xa.z", "xa,z", "xa:z", "xa\tz", "xa z" })
            Check(S("z", candidate) > mid, candidate);
    }

    private static void MultiTermQueryNeedsEveryTerm()
    {
        Check(PaletteMatcher.Score("rotar pagina", "Rotar página") is not null, "in order");
        Check(PaletteMatcher.Score("pagina rotar", "Rotar página") is not null, "any order");
        Check(PaletteMatcher.Score("  rotar   pagina ", "Rotar página") is not null, "extra spaces");
        Equal(null, PaletteMatcher.Score("rotar zzz", "Rotar página"), "a missing term");
    }

    private static void EmptyQueryMatchesEverythingEqually()
    {
        Equal(0, PaletteMatcher.Score("", "Rotate page"), "empty");
        Equal(0, PaletteMatcher.Score("   ", "Rotate page"), "blank");
        Equal(0, PaletteMatcher.Score("", ""), "both empty");
    }

    private static void NonEmptyQueryNeverMatchesEmptyCandidate() =>
        Equal(null, PaletteMatcher.Score("a", ""), "empty candidate");

    private static IReadOnlyList<(int Index, int Score)> Labels(string[] items) =>
        PaletteMatcher.Rank("pa", items, item => [item]);

    private static void RankDropsNonMatchesAndSortsByScore()
    {
        var ranked = Labels(["Spam", "Rotate", "Page", "Rotate page"]);
        Equal("2,3,0", string.Join(",", ranked.Select(r => r.Index)), "order");
        Check(ranked.Zip(ranked.Skip(1)).All(pair => pair.First.Score >= pair.Second.Score), "descending");
    }

    private static void RankTiesKeepCatalogOrder()
    {
        var ranked = Labels(["Page two", "Page one", "Page three"]);
        Equal("0,1,2", string.Join(",", ranked.Select(r => r.Index)), "ties");
        Check(ranked.All(r => r.Score == ranked[0].Score), "equal scores");
    }

    private static void RankEmptyQueryKeepsEverythingInCatalogOrder()
    {
        var ranked = PaletteMatcher.Rank("", new[] { "b", "a", "c" }, item => [item]);
        Equal("0:0,1:0,2:0", string.Join(",", ranked.Select(r => $"{r.Index}:{r.Score}")), "empty query");
    }

    private sealed record Entry(string Label, string English, string[] Keywords);

    private static IReadOnlyList<string> Haystacks(Entry entry) => [entry.Label, entry.English, .. entry.Keywords];

    private static void RankUsesTheBestHaystack()
    {
        var items = new[]
        {
            new Entry("Girar página", "Rotate page", ["turn"]),
            new Entry("Comprimir", "Compress", ["reducir tamaño", "mesh", "shrink"]),
        };
        Equal(0, PaletteMatcher.Rank("rotate", items, Haystacks).First().Index, "English label");
        Equal("1", string.Join(",", PaletteMatcher.Rank("tamano", items, Haystacks).Select(r => r.Index)), "accented keyword");
        var best = PaletteMatcher.Rank("sh", items, Haystacks);
        Equal(1, best.Count, "one hit");
        Equal((1, S("sh", "shrink")), best[0], "best haystack");
        Check(S("sh", "shrink") > S("sh", "mesh"), "shrink beats mesh");
    }

    private static readonly string[] Catalog = ["a", "b", "c", "d", "e"];

    private static string Ordered(PaletteRecents recents) => string.Join(",", recents.Order(Catalog, id => id));

    private static void NothingRecentKeepsTheGivenOrder() =>
        Equal("a,b,c,d,e", Ordered(new PaletteRecents()), "nothing recent");

    private static void TheMostRecentComesFirstThenTheRestInOrder()
    {
        var recents = new PaletteRecents();
        recents.Record("d");
        recents.Record("b");
        Equal("b,d,a,c,e", Ordered(recents), "recent first");
    }

    private static void RunningACommandAgainMovesItToTheFrontOnce()
    {
        var recents = new PaletteRecents();
        recents.Record("d");
        recents.Record("b");
        recents.Record("d");
        Equal("d,b,a,c,e", Ordered(recents), "again to the front");
    }

    private static void AnIdNoLongerInTheListIsIgnored()
    {
        var recents = new PaletteRecents();
        recents.Record("gone");
        recents.Record("c");
        Equal("c,a,b,d,e", Ordered(recents), "unknown id ignored");
    }
}
