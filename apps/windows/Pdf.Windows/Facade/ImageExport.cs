namespace Pdf.Windows.Facade;

public enum ImageExportFormat { Png, Jpeg }

/// <summary>Which pages an image export covers.</summary>
public enum ImageExportPages
{
    All,
    /// <summary>The page the viewer is showing.</summary>
    Current,
    /// <summary>Whatever was typed as a range, read by the core's grammar.</summary>
    Custom
}

/// <summary>What the Export images dialog asked for, before anything is checked.</summary>
public sealed record ImageExportRequest(
    ImageExportPages Pages,
    string CustomRange,
    uint CurrentPage,
    uint Dpi,
    ImageExportFormat Format);

/// <summary>One file an export will write: the page, and the name the core gave it.</summary>
public sealed record ImageExportFile(uint PageIndex, string FileName);

/// <summary>
/// An export every answer of which has been checked — the pages, their file
/// names, the resolution — so nothing left can be refused by the reader's
/// choices; only the disk or the renderer can still fail.
/// </summary>
public sealed record ImageExportPlan(IReadOnlyList<ImageExportFile> Files, uint Dpi, ImageExportFormat Format);

/// <summary>The resolutions an export offers, and the one it starts on.</summary>
/// <remarks>
/// The ceiling is 400, not the 600 an archival scan would suggest: the core
/// refuses any raster over 32 Mpx, and US Letter at 600 DPI is 5100x6600 =
/// 33.7 Mpx. A4 misses too. Offering a notch the two most common page sizes
/// cannot reach would be a control that lies. The GTK shell stops at the same
/// place for the same reason.
/// </remarks>
public static class ImageExportLimits
{
    public const uint MinDpi = 72;
    public const uint MaxDpi = 400;
    public const uint DefaultDpi = 150;
}
