using Pdf.Windows.Facade;
using Pdf.Windows.Localization;

namespace Pdf.Windows.Viewer;

/// <summary>What the status line says about an image export.</summary>
public static class ImageExportWording
{
    public static string FormatName(ImageExportFormat format) => format switch
    {
        ImageExportFormat.Jpeg => "JPEG",
        _ => "PNG",
    };

    public static string Summary(int count, ImageExportFormat format, string folder) =>
        I18n.T("export-summary-format", ("count", count), ("format", FormatName(format)), ("folder", folder));
}
