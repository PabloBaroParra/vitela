using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Resolves a form placement gesture in unrotated PDF page space.</summary>
internal static class FormPlacementRect
{
    public static PdfCoreRect Resolve(AnnotationPoint origin, AnnotationPoint end, PageDimensions page,
        double clickWidth, double clickHeight)
    {
        var (width, height) = page.Rotation is PageRotation.Clockwise90 or PageRotation.Clockwise270
            ? (page.HeightPt, page.WidthPt) : (page.WidthPt, page.HeightPt);
        if (width <= 0 || height <= 0) return new PdfCoreRect(0, 0, 0, 0);

        var startX = Math.Clamp(origin.X, 0, width);
        var startY = Math.Clamp(origin.Y, 0, height);
        var endX = Math.Clamp(end.X, 0, width);
        var endY = Math.Clamp(end.Y, 0, height);
        var dx = endX - startX;
        var dy = endY - startY;
        if (dx * dx + dy * dy < 64)
        {
            var boxWidth = Math.Min(clickWidth, width);
            var boxHeight = Math.Min(clickHeight, height);
            return new PdfCoreRect(Math.Clamp(startX, 0, width - boxWidth),
                Math.Clamp(startY - boxHeight, 0, height - boxHeight), boxWidth, boxHeight);
        }

        var x = Math.Min(startX, endX);
        var y = Math.Min(startY, endY);
        return new PdfCoreRect(x, y, Math.Min(Math.Max(4, Math.Abs(dx)), width - x),
            Math.Min(Math.Max(4, Math.Abs(dy)), height - y));
    }
}
