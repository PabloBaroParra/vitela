using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

/// <summary>
/// Replacing an image again before saving: the card offers Replace for an
/// image with a pending edit wherever the core may checkpoint, the facade
/// reports that, and the selection follows the image across the checkpoint
/// a second replacement may take.
/// </summary>
internal static class ImageReplaceAgainTests
{
    private static void Check(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }

    public static async Task RunAsync()
    {
        TheCardOnlyWaitsForASaveWhereNoCheckpointMayBeTaken();
        TheSelectionFollowsTheImageAcrossACheckpoint();
        await TheStateSaysWhetherContentEditsMayCheckpointAsync();
        await AnImageIsReplacedTwiceBeforeSavingAsync();
    }

    private static void TheCardOnlyWaitsForASaveWhereNoCheckpointMayBeTaken()
    {
        Check(ImageCardRules.Resolve(selected: false, pendingEdit: false, canCheckpoint: true) == ImageControls.Nothing,
            "No selection, no commands");
        Check(ImageCardRules.Resolve(selected: true, pendingEdit: false, canCheckpoint: false) == ImageControls.Ready,
            "An untouched image is replaceable anywhere");
        Check(ImageCardRules.Resolve(selected: true, pendingEdit: true, canCheckpoint: true) == ImageControls.Ready,
            "An edited image is replaceable again where the core may checkpoint");
        Check(ImageCardRules.Resolve(selected: true, pendingEdit: true, canCheckpoint: false) == ImageControls.PendingEdit,
            "Where it may not, the old save-first rule stands");
    }

    private static void TheSelectionFollowsTheImageAcrossACheckpoint()
    {
        static ContentImage Image(ulong id, double x, string? name) =>
            new(new PdfCoreContentImage(id, 0, new PdfCoreRect(x, 30, 100, 50), name), "s", 1);

        var held = Image(0, 20, null);
        var sameId = Image(0, 20, "Im1");
        Check(ImageCardRules.Current([Image(1, 300, "Im2"), sameId], held) == sameId, "Same id and box wins");

        // A checkpoint renumbers the page: the inline image the selection
        // held is now an XObject with another id, alone at the same box.
        var rebound = Image(4, 20, "VImg0");
        Check(ImageCardRules.Current([Image(0, 300, "Im2"), rebound], held) == rebound,
            "The one image at the same box is the selected one, renumbered");
        Check(ImageCardRules.Current([Image(3, 20, "A"), Image(4, 20, "B")], held) is null,
            "Two images at the same box are ambiguous: ask again");
        Check(ImageCardRules.Current([Image(0, 300, "Im2")], held) is null, "Nothing at the box: ask again");
    }

    private static async Task TheStateSaysWhetherContentEditsMayCheckpointAsync()
    {
        var core = new FakeCore { PageImages = [new(8, 0, new(20, 30, 100, 50), "Im1")] };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        Check(!(await facade.AnnotationStateAsync(session.SessionId)).Value!.ContentEditsCanCheckpoint,
            "The state follows the core: no checkpoint");
        core.CheckpointAvailable = true;
        Check((await facade.AnnotationStateAsync(session.SessionId)).Value!.ContentEditsCanCheckpoint,
            "The state follows the core: checkpoint");
    }

    private static async Task AnImageIsReplacedTwiceBeforeSavingAsync()
    {
        var core = new FakeCore { PageImages = [new(8, 0, new(20, 30, 100, 50), "Im1")], CheckpointAvailable = true };
        using var facade = new PdfDocumentFacade(core, new RecordingLogger());
        var session = (await facade.OpenAsync(new DocumentSource("image.pdf", [1]))).Value!;
        var first = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        var replaced = await facade.ReplaceContentImageAsync(session.SessionId, first, [1]);
        Check(replaced.IsSuccess && replaced.Value!.ContentEditsCanCheckpoint, "The first replacement records");
        var again = (await facade.PageImagesAsync(session.SessionId, 0)).Value![0];
        Check((await facade.PrepareImageReplacementAsync(session.SessionId, again)).IsSuccess, "The core is asked again, not the shell");
        Check((await facade.ReplaceContentImageAsync(session.SessionId, again, [2])).IsSuccess, "The second replacement records");
        Check(core.ImageReplacements.Count == 2 && core.ImageReplacements[1].After.SequenceEqual(new byte[] { 2 }),
            "Both replacements reach the core, the second with its own picture");
    }
}
