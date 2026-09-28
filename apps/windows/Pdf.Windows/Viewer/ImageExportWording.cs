using Pdf.Windows.Facade;

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
        $"Exported {count} {(count == 1 ? "page" : "pages")} as {FormatName(format)} to {folder}.";
}
