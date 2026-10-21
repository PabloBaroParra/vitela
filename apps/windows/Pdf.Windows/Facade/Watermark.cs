namespace Pdf.Windows.Facade;

/// <summary>Which pages the Add watermark dialog stamps.</summary>
public enum WatermarkPages
{
    All,
    Current,
    /// <summary>Whatever was typed in the range box, read by the core's grammar.</summary>
    Custom,
}

/// <summary>Whether a stamp is painted over the page content or behind it.</summary>
public enum WatermarkLayer
{
    OverContent,
    UnderContent,
}

/// <summary>What is painted: the same text, or the same picture, on every page.</summary>
public abstract record WatermarkMark;

/// <summary>Text in the core's font; the colour is the core's, not asked.</summary>
public sealed record TextMark(string Text, double FontSizePt) : WatermarkMark;

/// <summary>
/// A PNG or JPEG exactly as the reader picked it — decoding is the core's job.
/// <paramref name="ScalePercent"/> is how much of the page it may cover, asked
/// in percent like opacity.
/// </summary>
public sealed record PictureMark(byte[] Image, double ScalePercent) : WatermarkMark;

/// <summary>What the Add watermark dialog asked for, before anything is checked.</summary>
/// <param name="CurrentPage">Zero-based; clamped to the document, like export's.</param>
/// <param name="OpacityPercent">Asked in percent, as every image editor asks it.</param>
/// <param name="RotationDegrees">Counter-clockwise, as the reader sees the page.</param>
public sealed record WatermarkRequest(
    WatermarkPages Pages,
    string PageRange,
    uint CurrentPage,
    WatermarkMark Mark,
    double OpacityPercent,
    double RotationDegrees,
    WatermarkLayer Layer);

/// <summary>The text look the dialog starts from — the core's, in the dialog's units.</summary>
public sealed record WatermarkLook(double FontSizePt, double OpacityPercent, double RotationDegrees, WatermarkLayer Layer);

/// <summary>The picture look the dialog starts from — upright and behind, as the core has it.</summary>
public sealed record PictureWatermarkLook(double ScalePercent, double OpacityPercent, double RotationDegrees, WatermarkLayer Layer);

/// <summary>
/// A stamp the core recorded. <paramref name="Preview"/> is the refresh that
/// followed: a failed one leaves the stamp recorded and undoable, so it is
/// reported apart from the recording rather than as a refusal.
/// </summary>
public sealed record WatermarkAdded(IReadOnlyList<uint> Pages, OperationResult<AnnotationState> Preview);

/// <summary>
/// The dialog's ranges — the same as the GTK shell's
/// <c>content_edit::watermark::options</c>, so the two dialogs refuse the same
/// values.
/// </summary>
public static class WatermarkLimits
{
    /// <summary>The floor keeps the stamp legible; the ceiling is past what fits across portrait A4 at 45°.</summary>
    public const double MinFontSizePt = 8;
    public const double MaxFontSizePt = 200;

    /// <summary>5, not 1: the core refuses only a fully transparent stamp, but 1% is one nobody will find.</summary>
    public const double MinOpacityPercent = 5;
    public const double MaxOpacityPercent = 100;

    /// <summary>A picture's share of the page: the same "nobody would find it" floor; 100 touches two opposite edges.</summary>
    public const double MinScalePercent = 5;
    public const double MaxScalePercent = 100;

    public const double MinRotationDegrees = -180;
    public const double MaxRotationDegrees = 180;

    /// <summary>What the text box starts with — selected, so typing replaces it.</summary>
    public const string DefaultText = "DRAFT";
}
