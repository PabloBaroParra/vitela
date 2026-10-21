using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// Add watermark through the facade, the Windows twin of the GTK shell's
/// <c>content_edit::watermark::record</c> tests: one stamp is one undoable
/// content edit over the pages the dialog resolved, the look the reader chose
/// is what reaches the core, and every refusal leaves the log untouched.
/// </summary>
internal static class WatermarkParityTests
{
    public static async Task RunAsync()
    {
        await AStampIsOneEditOverTheChosenPagesAsync();
        await TheChosenLookIsWhatGetsRecordedAsync();
        await PageChoicesResolveLikeExportAsync();
        await BlankTextIsRefusedBeforeTheCoreAsync();
        await AnEncodingGapIsNamedAndRecordsNothingAsync();
        await ABadRangeSurfacesTheCoresSentenceAsync();
        await AForbiddenDocumentIsRefusedAsync();
        await AStaleSessionIsRefusedAsync();
        await UndoTakesTheWholeStampBackAsync();
        await APictureIsRecordedUntouchedWithItsScaleAndLayerAsync();
        await ATextStampCarriesTheLayerTheReaderChoseAsync();
        await AFileThatIsNotAPictureIsRefusedInWordsAndRecordsNothingAsync();
        await NoPictureChosenIsRefusedBeforeTheCoreAsync();
        await APictureStampIsOneUndoStepAsync();
        await TheStampSurvivesAFailedPreviewAsync();
        TheDefaultLookComesFromTheCoreInPercent();
        TheDefaultLookSitsInsideTheDialogsRanges();
        ThePictureDefaultLookComesFromTheCore();
        TheSummaryCountsPagesInWords();
    }

    private static async Task<(FakeCore Core, PdfDocumentFacade Facade, string Session)> OpenAsync(FakeCore? core = null)
    {
        core ??= new FakeCore { PageCount = 3 };
        var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("watermark.pdf", [1]))).Value!;
        return (core, facade, session.SessionId);
    }

    private static WatermarkRequest Request(
        WatermarkPages pages = WatermarkPages.All,
        string range = "",
        uint current = 0,
        string text = "DRAFT") =>
        new(pages, range, current, new TextMark(text, DefaultLook.FontSizePt), DefaultLook.OpacityPercent, DefaultLook.RotationDegrees, DefaultLook.Layer);

    private static WatermarkRequest PictureRequest(byte[]? image = null, double scalePercent = 80, WatermarkPages pages = WatermarkPages.All) =>
        new(pages, "", 0, new PictureMark(image ?? FakeCore.ValidPicture, scalePercent),
            DefaultPictureLook.OpacityPercent, DefaultPictureLook.RotationDegrees, DefaultPictureLook.Layer);

    private static readonly WatermarkLook DefaultLook = new PdfDocumentFacade(new FakeCore(), new RecordingLogger()).DefaultWatermarkLook();
    private static readonly PictureWatermarkLook DefaultPictureLook = new PdfDocumentFacade(new FakeCore(), new RecordingLogger()).DefaultPictureWatermarkLook();

    private static async Task AStampIsOneEditOverTheChosenPagesAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var added = await facade.AddWatermarkAsync(session, Request());

        Check(added.IsSuccess, "a valid stamp is recorded");
        Check(added.Value!.Pages.SequenceEqual([0u, 1u, 2u]), "all pages names every page in order");
        Check(added.Value.Preview.IsSuccess && added.Value.Preview.Value!.CanUndo, "the recorded stamp is undoable");
        var stamp = core.Watermarks.Single().Watermark;
        Check(stamp.Pages.SequenceEqual([0u, 1u, 2u]) && stamp.Text == "DRAFT", "one edit carries every page and the text as typed");
        Check(stamp.Color == FakeCore.DefaultWatermarkColor, "the colour is the core's default, not a shell copy");
        Check(core.RefreshPreviewCalls == 1, "the preview is rebuilt once, however many pages");
    }

    private static async Task TheChosenLookIsWhatGetsRecordedAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var chosen = Request(WatermarkPages.Current, current: 1) with { Mark = new TextMark(" COPY", 24), OpacityPercent = 50, RotationDegrees = -30 };
        Check((await facade.AddWatermarkAsync(session, chosen)).IsSuccess, "a custom look is accepted");

        var stamp = core.Watermarks.Single().Watermark;
        Check((stamp.FontSizePt, stamp.Opacity, stamp.RotationDegrees) == (24, 0.5, -30), "size, opacity (from percent) and angle reach the core");
        Check(stamp.Text == " COPY", "leading spaces are kept as typed");
    }

    private static async Task PageChoicesResolveLikeExportAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, ParsedSelection = [0, 2] });
        using var disposeFacade = facade;

        var current = await facade.AddWatermarkAsync(session, Request(WatermarkPages.Current, current: 7));
        Check(current.Value!.Pages.SequenceEqual([2u]), "the current page is clamped to the document");

        var custom = await facade.AddWatermarkAsync(session, Request(WatermarkPages.Custom, "1,3"));
        Check(custom.Value!.Pages.SequenceEqual([0u, 2u]), "a custom range is the core's grammar's answer");
        Check(core.LastSelection == ("1,3", 3u), "the typed range and the page count reach the core");
    }

    private static async Task BlankTextIsRefusedBeforeTheCoreAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        foreach (var blank in new[] { "", "   " })
        {
            var refused = await facade.AddWatermarkAsync(session, Request(text: blank));
            Check(!refused.IsSuccess && refused.Error!.Message == "Type the text to stamp on the pages.", "blank text is refused in words");
        }
        var emptyRange = await facade.AddWatermarkAsync(session, Request(WatermarkPages.Custom, " "));
        Check(!emptyRange.IsSuccess && emptyRange.Error!.Message == "Type which pages to stamp, for example 1-3,7.", "an empty range asks for one");
        Check(core.Watermarks.Count == 0 && !core.LastDocument!.CanUndo, "nothing is recorded");
    }

    private static async Task AnEncodingGapIsNamedAndRecordsNothingAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, UnencodableCharacter = "草" });
        using var disposeFacade = facade;

        var refused = await facade.AddWatermarkAsync(session, Request(text: "草稿"));

        Check(!refused.IsSuccess && refused.Error!.Message.Contains("\"草\"", StringComparison.Ordinal), "the character the font cannot show is named");
        Check(!refused.Error!.Message.Contains("Helvetica", StringComparison.Ordinal), "no PDF font name reaches the reader");
        Check(core.Watermarks.Count == 0 && core.RefreshPreviewCalls == 0, "nothing is recorded or refreshed");
    }

    private static async Task ABadRangeSurfacesTheCoresSentenceAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, SelectionRefusal = "There is no page 9 in this 3-page document." });
        using var disposeFacade = facade;

        var refused = await facade.AddWatermarkAsync(session, Request(WatermarkPages.Custom, "9"));

        Check(!refused.IsSuccess && refused.Error!.Message == "There is no page 9 in this 3-page document.", "the core's own sentence is shown");
        Check(core.Watermarks.Count == 0, "nothing is recorded");
    }

    private static async Task AForbiddenDocumentIsRefusedAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        core.LastDocument!.ContentEditingAllowed = false;

        var refused = await facade.AddWatermarkAsync(session, Request());

        Check(!refused.IsSuccess && refused.Error!.Message == "This document does not permit content changes.", "the content-edit permission gates the stamp");
        Check(core.Watermarks.Count == 0, "nothing is recorded");
    }

    private static async Task AStaleSessionIsRefusedAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var refused = await facade.AddWatermarkAsync(session + "-gone", Request());

        Check(!refused.IsSuccess && refused.Error!.Message == "The document is no longer available.", "a session that is gone is refused");
        Check(core.Watermarks.Count == 0, "nothing is recorded");
    }

    private static async Task UndoTakesTheWholeStampBackAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        await facade.AddWatermarkAsync(session, Request());

        var undone = await facade.UndoAsync(session);

        Check(undone.IsSuccess && !undone.Value!.CanUndo && undone.Value.CanRedo, "one undo takes the whole stamp back");
        Check(core.Watermarks.Count == 1, "undo did not ask for a second stamp");
    }

    private static async Task APictureIsRecordedUntouchedWithItsScaleAndLayerAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        byte[] picture = [0x89, 1, 2, 3];

        var added = await facade.AddWatermarkAsync(session, PictureRequest(picture, 80));

        Check(added.IsSuccess && added.Value!.Pages.SequenceEqual([0u, 1u, 2u]), "a picture is stamped on every chosen page");
        Check(core.Watermarks.Count == 0, "a picture is not recorded as a text stamp");
        var stamp = core.ImageWatermarks.Single().Watermark;
        Check(stamp.Image.SequenceEqual(picture), "the picked bytes reach the core untouched");
        Check(stamp.Scale == 0.8, "the size is asked in percent of the page and recorded as a fraction");
        Check((stamp.Opacity, stamp.RotationDegrees, stamp.Layer) == (0.3, 0.0, WatermarkLayer.UnderContent), "the look is the core's picture look");
        Check(core.RefreshPreviewCalls == 1, "the preview is rebuilt once");
    }

    private static async Task ATextStampCarriesTheLayerTheReaderChoseAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var behind = Request() with { Layer = WatermarkLayer.UnderContent };
        Check((await facade.AddWatermarkAsync(session, behind)).IsSuccess, "text behind the content is accepted");

        Check(core.Watermarks.Single().Watermark.Layer == WatermarkLayer.UnderContent, "the layer reaches the core instead of a hardcoded over");
    }

    private static async Task AFileThatIsNotAPictureIsRefusedInWordsAndRecordsNothingAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var refused = await facade.AddWatermarkAsync(session, PictureRequest([0x25, 0x50, 0x44, 0x46]));

        Check(!refused.IsSuccess && refused.Error!.Message == "That file could not be read as a PNG or JPEG picture. Choose another one.", "an undecodable file is refused in plain words");
        Check(core.ImageWatermarks.Count == 0 && core.RefreshPreviewCalls == 0 && !core.LastDocument!.CanUndo, "nothing is recorded or refreshed");
    }

    private static async Task NoPictureChosenIsRefusedBeforeTheCoreAsync()
    {
        var (core, facade, session) = await OpenAsync();
        using var disposeFacade = facade;

        var refused = await facade.AddWatermarkAsync(session, PictureRequest([]));

        Check(!refused.IsSuccess && refused.Error!.Message == "Choose a PNG or JPEG picture to stamp on the pages.", "no picture asks for one");
        Check(core.ImageWatermarks.Count == 0 && core.ImageWatermarkAttempts == 0, "the core is not asked");
    }

    private static async Task APictureStampIsOneUndoStepAsync()
    {
        var (_, facade, session) = await OpenAsync();
        using var disposeFacade = facade;
        await facade.AddWatermarkAsync(session, PictureRequest());

        var undone = await facade.UndoAsync(session);

        Check(undone.IsSuccess && !undone.Value!.CanUndo && undone.Value.CanRedo, "one undo takes the whole picture stamp back");
    }

    private static void ThePictureDefaultLookComesFromTheCore()
    {
        Check(DefaultPictureLook == new PictureWatermarkLook(50, 30, 0, WatermarkLayer.UnderContent), "upright, half the page, faded, behind — as percent");
        Check(DefaultLook.Layer == WatermarkLayer.OverContent, "the text look starts over the content");
        Check(DefaultPictureLook.ScalePercent is >= WatermarkLimits.MinScalePercent and <= WatermarkLimits.MaxScalePercent, "the default size fits the range");
        Check(DefaultPictureLook.RotationDegrees is >= WatermarkLimits.MinRotationDegrees and <= WatermarkLimits.MaxRotationDegrees, "the default picture angle fits the range");
    }

    private static async Task TheStampSurvivesAFailedPreviewAsync()
    {
        var (core, facade, session) = await OpenAsync(new FakeCore { PageCount = 3, RefreshPreviewThrows = true });
        using var disposeFacade = facade;

        var added = await facade.AddWatermarkAsync(session, Request());

        Check(added.IsSuccess && !added.Value!.Preview.IsSuccess, "a failed refresh is reported apart from the recording");
        Check(core.Watermarks.Count == 1 && core.LastDocument!.CanUndo, "the stamp stays recorded and undoable");
    }

    private static void TheDefaultLookComesFromTheCoreInPercent()
    {
        Check(DefaultLook == new WatermarkLook(60, 30, 45, WatermarkLayer.OverContent), "the core's 0.3 opacity is asked as 30 percent");
    }

    /// <summary>
    /// Against the fake's copy of <c>Watermark::diagonal</c> (60 pt, 0.3, 45°):
    /// the dialog's ranges must hold the look the core starts from.
    /// </summary>
    private static void TheDefaultLookSitsInsideTheDialogsRanges()
    {
        Check(DefaultLook.FontSizePt is >= WatermarkLimits.MinFontSizePt and <= WatermarkLimits.MaxFontSizePt, "the default size fits the range");
        Check(DefaultLook.OpacityPercent is >= WatermarkLimits.MinOpacityPercent and <= WatermarkLimits.MaxOpacityPercent, "the default opacity fits the range");
        Check(DefaultLook.RotationDegrees is >= WatermarkLimits.MinRotationDegrees and <= WatermarkLimits.MaxRotationDegrees, "the default angle fits the range");
    }

    private static void TheSummaryCountsPagesInWords()
    {
        Check(WatermarkWording.Summary(1) == "Watermark added to 1 page. Save to keep the change.", "one page in the singular");
        Check(WatermarkWording.Summary(4) == "Watermark added to 4 pages. Save to keep the change.", "many pages in the plural");
    }

    private static void Check(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }
}

// The core is faked at the boundary: it records the stamp, refuses the
// configured character, and hands back a copy of Watermark::diagonal's look.
sealed partial class FakeCore
{
    public static readonly PdfCoreColor DefaultWatermarkColor = new(128, 128, 128);
    public List<PdfCoreEdit.AddWatermark> Watermarks { get; } = [];
    public List<PdfCoreEdit.AddImageWatermark> ImageWatermarks { get; } = [];
    public int ImageWatermarkAttempts { get; private set; }

    /// <summary>A PNG's first byte, which is all the fake decoder looks at; anything else is not a picture.</summary>
    public static readonly byte[] ValidPicture = [0x89, 0x50, 0x4E, 0x47];

    public PdfCoreWatermark DefaultWatermark(IReadOnlyList<uint> pages, string text) =>
        new(pages, text, 60, DefaultWatermarkColor, 0.3, 45, WatermarkLayer.OverContent);

    // A copy of Watermark::picture's look: upright, half the page, faded, behind.
    public PdfCoreImageWatermark DefaultImageWatermark(IReadOnlyList<uint> pages, byte[] image) =>
        new(pages, image, 0.5, 0.3, 0, WatermarkLayer.UnderContent);

    private bool TryApplyWatermark(FakeDocument fake, PdfCoreEdit edit)
    {
        if (edit is PdfCoreEdit.AddImageWatermark picture)
        {
            ImageWatermarkAttempts++;
            if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
            if (picture.Watermark.Image is not [0x89, ..]) throw new PdfCoreException(PdfCoreError.InvalidImage, "InvalidImage");
            ImageWatermarks.Add(picture);
            fake.Apply(edit);
            return true;
        }
        if (edit is not PdfCoreEdit.AddWatermark stamp) return false;
        if (!fake.ContentEditingAllowed) throw new PdfCoreException(PdfCoreError.UnsupportedOperation, "content editing is not permitted");
        if (UnencodableCharacter is { } character && stamp.Watermark.Text.Contains(character, StringComparison.Ordinal))
            throw new PdfCoreException(PdfCoreError.EncodingGap, "EncodingGap", character);
        Watermarks.Add(stamp);
        fake.Apply(edit);
        return true;
    }
}
