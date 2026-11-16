using System.Xml.Linq;
using Pdf.Windows.Viewer;

/// <summary>
/// The command palette's catalog: one entry per command the Windows shell
/// has, English titles with English and Spanish search synonyms, shortcut
/// hints from the shared table, and availability that mirrors each button's
/// gate. Ported from the GTK shell's <c>palette/entries/tests.rs</c>.
/// </summary>
internal static class PaletteCatalogTests
{
    /// <summary>Most synonyms one entry may carry per language (T-231).</summary>
    private const int MaxKeywords = 10;

    /// <summary>Most stray hits a common, unspecific query may return.</summary>
    private const int MaxStrayHits = 4;

    /// <summary>The entries genuinely about "page"/"pagina".</summary>
    private static readonly string[] PageRelevant = ["mode.organize", "pages.extract", "pages.split"];

    /// <summary>The entries genuinely about "pdf": their result is a PDF other than the open one.</summary>
    private static readonly string[] PdfRelevant = ["file.new", "pages.add-pdfs"];

    public static Task RunAsync()
    {
        EveryCommandHasExactlyOneEntry();
        IdsAreUnique();
        CategoriesAreContiguous();
        EveryEntryHasATitleAndShortSynonymsInBothLanguages();
        ShortcutEntriesShowTheSharedTable();
        EveryShortcutInTheTableIsBoundInTheWindow();
        HomeCardListsTheFileKeysAndTheCommandPalette();
        AnEmptyQueryKeepsCatalogOrder();
        SpanishWithoutAccentsFindsItsCommands();
        EnglishTitlesFindTheirCommands();
        SpanishKeywordsFindTheirCommands();
        EnglishKeywordsFindTheirCommands();
        SpanishKeywordsAreSearchedInTheEnglishUi();
        CommonQueriesStayNarrow();
        TheNarrownessGuardCatchesAPollutedCatalog();
        WithoutADocumentOnlyNavigationAndDocumentFreeCommandsRun();
        AnAnnotationRefusalReachesEveryAnnotationEntry();
        AnAllowedDocumentMakesEveryDocumentCommandAvailable();
        AStillOpeningDocumentRefusesPermissionGatedEntries();
        UndoAndRedoFollowTheHistory();
        ProtectFollowsThePermissionItsButtonAsks();
        ContentAndFormRefusalsUseTheirButtonsReasons();
        SplitNeedsTwoPages();
        SigningEntriesStateTheSigningRefusal();
        OrganizeAsksTheOpenRefusal();
        FindAsksTheTextExtractionRefusal();
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

    private static PaletteEntry Entry(PaletteCommand command) => PaletteCatalog.Entries.Single(entry => entry.Command == command);

    private static List<string> Found(string query) =>
        PaletteCatalog.Search(query, PaletteContext.NoDocument).Select(found => found.Entry.Id).ToList();

    /// <summary>A one-page document that allows everything, with its permission snapshots loaded.</summary>
    private static PaletteContext Allowed(int pages = 1) => new(
        HasDocument: true,
        PageCount: pages,
        AnnotationEditingAllowed: true,
        ContentEditingAllowed: true,
        FormStructureAllowed: true,
        SigningLoaded: true,
        SigningRefusal: null,
        CanUndo: true,
        CanRedo: true,
        TextExtractionRefusal: null);

    private static string? Refusal(PaletteCommand command, PaletteContext context) => Entry(command).Refusal(context);

    private static void EveryCommandHasExactlyOneEntry()
    {
        foreach (var command in Enum.GetValues<PaletteCommand>())
            Equal(1, PaletteCatalog.Entries.Count(entry => entry.Command == command), command.ToString());
    }

    private static void IdsAreUnique() =>
        Equal(PaletteCatalog.Entries.Count, PaletteCatalog.Entries.Select(entry => entry.Id).Distinct().Count(), "unique ids");

    private static void CategoriesAreContiguous()
    {
        var seen = new List<PaletteCategory>();
        foreach (var entry in PaletteCatalog.Entries)
        {
            if (seen.Count > 0 && seen[^1] == entry.Category) continue;
            Check(!seen.Contains(entry.Category), $"{entry.Category} is split in two");
            seen.Add(entry.Category);
        }
    }

    private static void EveryEntryHasATitleAndShortSynonymsInBothLanguages()
    {
        foreach (var entry in PaletteCatalog.Entries)
        {
            Check(entry.Title.Length > 0 && entry.SpanishTitle.Length > 0, $"{entry.Id} titles");
            foreach (var (language, keywords) in new[] { ("English", entry.Keywords), ("Spanish", entry.SpanishKeywords) })
            {
                var words = keywords.Split(' ', StringSplitOptions.RemoveEmptyEntries).Length;
                Check(words is >= 1 and <= MaxKeywords, $"{entry.Id} has {words} {language} keywords");
            }
        }
    }

    private static void ShortcutEntriesShowTheSharedTable()
    {
        foreach (var (command, shortcut) in new[]
        {
            (PaletteCommand.Open, ShellShortcuts.Open), (PaletteCommand.New, ShellShortcuts.New),
            (PaletteCommand.Save, ShellShortcuts.Save), (PaletteCommand.Print, ShellShortcuts.Print),
            (PaletteCommand.Undo, ShellShortcuts.Undo), (PaletteCommand.Redo, ShellShortcuts.Redo),
            (PaletteCommand.Find, ShellShortcuts.Find),
        })
            Equal(shortcut, Entry(command).Shortcut, command.ToString());
        Equal(null, Entry(PaletteCommand.Highlight).Shortcut, "a tool has no shortcut");
    }

    /// <summary>The key a palette row or the Home card prints is a key the window listens for.</summary>
    private static void EveryShortcutInTheTableIsBoundInTheWindow()
    {
        XNamespace presentation = "http://schemas.microsoft.com/winfx/2006/xaml/presentation";
        var bound = XDocument.Load(FindRepoFile("apps", "windows", "Pdf.Windows", "MainWindow.xaml"))
            .Descendants(presentation + "KeyboardAccelerator")
            .Select(accelerator => ((string?)accelerator.Attribute("Key"), (string?)accelerator.Attribute("Modifiers")))
            .ToHashSet();
        foreach (var shortcut in ShellShortcuts.All)
            Check(bound.Contains((shortcut.Key.ToString(), "Control")), $"{shortcut.Label} ({shortcut.Name}) is not bound in MainWindow.xaml");
        Equal(ShellShortcuts.All.Count, ShellShortcuts.All.Select(shortcut => shortcut.Key).Distinct().Count(), "one key per shortcut");
    }

    /// <summary>Home's shortcut card (GTK T-232): the five file and search keys it always listed, plus Ctrl+K.</summary>
    private static void HomeCardListsTheFileKeysAndTheCommandPalette()
    {
        Equal("Open Ctrl+O, New Ctrl+N, Save Ctrl+S, Find Ctrl+F, Print Ctrl+P, Command palette Ctrl+K",
            string.Join(", ", ShellShortcuts.HomeCard.Select(shortcut => $"{shortcut.Name} {shortcut.Label}")), "Home card rows");
        Check(ShellShortcuts.HomeCard.All(ShellShortcuts.All.Contains), "every card row is a bound shortcut");
    }

    private static void AnEmptyQueryKeepsCatalogOrder() =>
        Check(Found("").SequenceEqual(PaletteCatalog.Entries.Select(entry => entry.Id)), "empty query keeps catalog order");

    private static void SpanishWithoutAccentsFindsItsCommands()
    {
        Equal("pages.add-pdfs", Found("anadir pdf").FirstOrDefault(), "anadir pdf");
        Equal("document.export-images", Found("imagenes").FirstOrDefault(), "imagenes");
        Equal("edit.undo", Found("deshacer").FirstOrDefault(), "deshacer: the Spanish title");
    }

    private static void EnglishTitlesFindTheirCommands()
    {
        Equal("annotate.highlight", Found("highlight").FirstOrDefault(), "highlight");
        Equal("document.watermark", Found("watermark").FirstOrDefault(), "watermark");
    }

    private static void SpanishKeywordsFindTheirCommands()
    {
        var pagina = Found("pagina");
        foreach (var id in PageRelevant) Check(pagina.Contains(id), $"pagina misses {id}: {string.Join(",", pagina)}");
        Equal("mode.organize", pagina.FirstOrDefault(), "pagina first");
        Equal("pages.add-pdfs", Found("unir pdf").FirstOrDefault(), "unir pdf");
        Equal("document.compress", Found("reducir").FirstOrDefault(), "reducir");
        var firma = Found("firma");
        Check(firma.FirstOrDefault() is { } first && (first.StartsWith("sign.", StringComparison.Ordinal) || first == "mode.sign"), $"firma: {string.Join(",", firma)}");
        Check(firma.Contains("sign.draw"), $"firma: {string.Join(",", firma)}");
    }

    private static void EnglishKeywordsFindTheirCommands()
    {
        Equal("pages.add-pdfs", Found("merge").FirstOrDefault(), "merge");
        Equal("document.compress", Found("shrink").FirstOrDefault(), "shrink");
    }

    /// <summary>
    /// Windows has no Spanish UI, so unlike GTK (active language plus English)
    /// both synonym lists are always searched: a Spanish speaker can find a
    /// command by the word they think in.
    /// </summary>
    private static void SpanishKeywordsAreSearchedInTheEnglishUi() =>
        Check(Found("fusionar").Contains("pages.add-pdfs"), "fusionar finds Add PDFs");

    /// <summary>
    /// "pdf" and "pagina" keep GTK's cap. English "page" gets two more
    /// strays: GTK only ever searched it with the English synonyms, while
    /// Windows always searches both lists, and two Spanish ones hold a
    /// scattered p-a-g-e ("Proteger … seguridad permisos", "desplegable …").
    /// The relevant entries still rank first.
    /// </summary>
    private static void CommonQueriesStayNarrow()
    {
        foreach (var (query, relevant, maxStrays) in new[] { ("pdf", PdfRelevant, MaxStrayHits), ("page", PageRelevant, MaxStrayHits + 2), ("pagina", PageRelevant, MaxStrayHits) })
        {
            var found = Found(query);
            if (NarrownessViolation(found, relevant, maxStrays) is { } violation)
                throw new InvalidOperationException($"\"{query}\": {violation}: {string.Join(",", found)}");
        }
    }

    /// <summary>
    /// Why <paramref name="found"/> fails the narrowness rule, or null: every
    /// relevant entry is returned, all rank ahead of every stray, and at most
    /// <see cref="MaxStrayHits"/> strays.
    /// </summary>
    private static string? NarrownessViolation(IReadOnlyList<string> found, IReadOnlyList<string> relevant, int maxStrays = MaxStrayHits)
    {
        if (relevant.FirstOrDefault(id => !found.Contains(id)) is { } missing) return $"{missing} is not returned";
        for (var position = 0; position < Math.Min(relevant.Count, found.Count); position++)
            if (!relevant.Contains(found[position])) return $"stray {found[position]} ranks at {position}, ahead of a relevant entry";
        var strays = found.Count - relevant.Count;
        return strays > maxStrays ? $"{strays} stray hits" : null;
    }

    private static void TheNarrownessGuardCatchesAPollutedCatalog()
    {
        static List<string> Rank(string[] labels) =>
            PaletteMatcher.Rank("page", labels, label => [label]).Select(r => labels[r.Index]).ToList();
        string[] relevant = ["Extract pages"];
        Equal(null, NarrownessViolation(Rank(["Highlight marker", "Extract pages", "Export images"]), relevant), "clean");
        Check(NarrownessViolation(Rank(["Highlight marker page", "Extract pages"]), relevant)?.Contains("ranks at") == true, "polluted");
        Check(NarrownessViolation(Rank(["Extract pages", "Export images", "Stamp approved image", "Shape rectangle arrow line", "Spread grid edges", "Spare edge"]), relevant)
            ?.Contains("stray hits") == true, "scattered");
    }

    private static void WithoutADocumentOnlyNavigationAndDocumentFreeCommandsRun()
    {
        var none = PaletteContext.NoDocument;
        foreach (var entry in PaletteCatalog.Entries.Where(entry => entry.Category == PaletteCategory.Mode))
            Equal(null, entry.Refusal(none), $"{entry.Id} must stay available: the rail picks a file first");
        foreach (var command in new[] { PaletteCommand.Open, PaletteCommand.New, PaletteCommand.Compress })
            Equal(null, Refusal(command, none), command.ToString());
        foreach (var (command, reason) in new[]
        {
            (PaletteCommand.Save, "Open a PDF before saving."),
            (PaletteCommand.Print, "Open a PDF before printing."),
            (PaletteCommand.Find, "Open a PDF before searching."),
            (PaletteCommand.Highlight, "Open a PDF before annotating."),
            (PaletteCommand.TextField, "Open a PDF before adding form fields."),
            (PaletteCommand.ExportImages, "Open a PDF before exporting it."),
            (PaletteCommand.Watermark, "Open a PDF before adding a watermark."),
            (PaletteCommand.AddPdfs, "Open a PDF before organizing its pages."),
            (PaletteCommand.Extract, "Open a PDF before extracting pages."),
            (PaletteCommand.Split, "Open a PDF before splitting it."),
            (PaletteCommand.SignWithFile, "Open a PDF before signing."),
            (PaletteCommand.DrawSignature, "Open a PDF before signing."),
        })
            Equal(reason, Refusal(command, none), command.ToString());
    }

    private static readonly PaletteCommand[] AnnotationTools =
    [
        PaletteCommand.Highlight, PaletteCommand.Underline, PaletteCommand.Strikeout, PaletteCommand.Ink,
        PaletteCommand.Note, PaletteCommand.TextBox, PaletteCommand.Shape, PaletteCommand.Stamp,
    ];

    private static void AnAnnotationRefusalReachesEveryAnnotationEntry()
    {
        var forbidden = Allowed() with { AnnotationEditingAllowed = false };
        foreach (var command in AnnotationTools.Append(PaletteCommand.DrawSignature))
            Equal(PaletteCatalog.AnnotationRefusal, Refusal(command, forbidden), command.ToString());
        Equal(null, Refusal(PaletteCommand.ModeAnnotate, forbidden), "navigation is never refused: the page states it");
    }

    private static void AnAllowedDocumentMakesEveryDocumentCommandAvailable()
    {
        foreach (var entry in PaletteCatalog.Entries)
            Equal(null, entry.Refusal(Allowed(pages: 2)), entry.Id);
    }

    private static void AStillOpeningDocumentRefusesPermissionGatedEntries()
    {
        var opening = Allowed() with { AnnotationEditingAllowed = null, FormStructureAllowed = null, SigningLoaded = false };
        foreach (var command in new[] { PaletteCommand.Highlight, PaletteCommand.Watermark, PaletteCommand.TextField, PaletteCommand.SignWithToken })
            Equal(PaletteCatalog.StillOpening, Refusal(command, opening), command.ToString());
        Equal(null, Refusal(PaletteCommand.Save, opening), "Save asks only for pages");
    }

    private static void UndoAndRedoFollowTheHistory()
    {
        var clean = Allowed() with { CanUndo = false, CanRedo = false };
        Equal("Nothing to undo.", Refusal(PaletteCommand.Undo, clean), "undo");
        Equal("Nothing to redo.", Refusal(PaletteCommand.Redo, clean), "redo");
        Equal(null, Refusal(PaletteCommand.Undo, Allowed()), "undo available");
    }

    private static void ProtectFollowsThePermissionItsButtonAsks()
    {
        var locked = Allowed() with { ContentEditingAllowed = false };
        Equal("This document does not permit changes to its protection.", Refusal(PaletteCommand.ModeProtect, locked), "protect");
        Equal(null, Refusal(PaletteCommand.ModeEdit, locked), "Edit is navigation");
    }

    private static void ContentAndFormRefusalsUseTheirButtonsReasons()
    {
        Equal("This document does not permit content changes.", Refusal(PaletteCommand.Watermark, Allowed() with { ContentEditingAllowed = false }), "watermark");
        Equal("This document does not permit creating form fields.", Refusal(PaletteCommand.Dropdown, Allowed() with { FormStructureAllowed = false }), "forms");
    }

    private static void SplitNeedsTwoPages()
    {
        Equal("Split needs a document with at least two pages.", Refusal(PaletteCommand.Split, Allowed(pages: 1)), "one page");
        Equal(null, Refusal(PaletteCommand.Extract, Allowed(pages: 1)), "extract one page");
    }

    private static void SigningEntriesStateTheSigningRefusal()
    {
        var refused = Allowed() with { SigningRefusal = "This document does not permit signing." };
        foreach (var command in new[] { PaletteCommand.SignWithFile, PaletteCommand.SignWithToken, PaletteCommand.SignWithSystemCertificate })
            Equal("This document does not permit signing.", Refusal(command, refused), command.ToString());
        Equal(null, Refusal(PaletteCommand.DrawSignature, refused), "drawing asks the annotation permission");
    }

    /// <summary>
    /// GTK <c>organize::open_refusal</c>: opening Organize asks the content
    /// permission, worded as the Edit page's own content refusal.
    /// </summary>
    private static void OrganizeAsksTheOpenRefusal()
    {
        Equal(PaletteCatalog.ContentRefusal, Refusal(PaletteCommand.ModeOrganize, Allowed() with { ContentEditingAllowed = false }), "organize refused");
        Equal(null, Refusal(PaletteCommand.ModeOrganize, PaletteContext.NoDocument), "no document: the rail picks one first");
        // The one gate every door into Organize asks (GTK organize::open_refusal).
        Equal(PaletteCatalog.ContentRefusal, PaletteCatalog.OrganizeOpenRefusal(contentEditingAllowed: false), "the door refuses");
        Equal(null, PaletteCatalog.OrganizeOpenRefusal(contentEditingAllowed: true), "the door opens");
    }

    /// <summary>GTK <c>text_extraction_refusal</c>: Find states the search's own refusal.</summary>
    private static void FindAsksTheTextExtractionRefusal()
    {
        const string refusal = "This document does not permit copying or extracting its text.";
        Equal(refusal, Refusal(PaletteCommand.Find, Allowed() with { TextExtractionRefusal = refusal }), "find refused");
        Equal("Open a PDF before searching.", Refusal(PaletteCommand.Find, PaletteContext.NoDocument with { TextExtractionRefusal = refusal }), "no document first");
    }

    private static string FindRepoFile(params string[] parts)
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var candidate = Path.Combine([directory.FullName, .. parts]);
            if (File.Exists(candidate)) return candidate;
        }
        throw new FileNotFoundException($"{string.Join('/', parts)} was not found above the test binaries");
    }
}
