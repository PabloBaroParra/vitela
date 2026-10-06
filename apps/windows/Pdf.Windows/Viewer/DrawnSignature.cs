namespace Pdf.Windows.Viewer;

/// <summary>A point on the signature pad, in pad pixels (DIPs).</summary>
public readonly record struct PadPoint(double X, double Y);

/// <summary>
/// The part of the pad the PNG covers, in pad pixels: the ink's bounds grown by
/// half a stroke so round caps are not clipped. <see cref="Scale"/> shrinks it
/// so the long side fits the cap; a small signature is never enlarged.
/// </summary>
public readonly record struct SignatureFrame(double Left, double Top, double Width, double Height, double Scale)
{
    public int PixelWidth => Math.Max(1, (int)Math.Ceiling(Width * Scale));
    public int PixelHeight => Math.Max(1, (int)Math.Ceiling(Height * Scale));
}

/// <summary>
/// Geometry of a drawn signature, free of any UI type so it can be tested
/// without a window. The same rules as the Android shell's
/// <c>DrawnSignature.kt</c>: the PNG is cropped to the ink, so the core's stamp
/// placement sizes the signature and not the empty pad around it.
/// </summary>
public static class DrawnSignature
{
    /// <summary>Longest side of the PNG: the core places a stamp at most 144 pt long, so this is already ~600 DPI.</summary>
    public const int MaxSidePx = 1200;

    /// <summary>A stroke needs two points to leave a line; a bare click draws nothing.</summary>
    public static bool HasInk(IReadOnlyList<IReadOnlyList<PadPoint>> strokes) => strokes.Any(stroke => stroke.Count >= 2);

    /// <summary>The crop around the ink, or null while the pad holds no line.</summary>
    public static SignatureFrame? Frame(IReadOnlyList<IReadOnlyList<PadPoint>> strokes, double strokeWidth, int maxSidePx = MaxSidePx)
    {
        var points = strokes.Where(stroke => stroke.Count >= 2).SelectMany(stroke => stroke).ToList();
        if (points.Count == 0) return null;
        var half = strokeWidth / 2;
        var left = points.Min(point => point.X) - half;
        var top = points.Min(point => point.Y) - half;
        var width = points.Max(point => point.X) + half - left;
        var height = points.Max(point => point.Y) + half - top;
        var scale = Math.Min(1.0, maxSidePx / Math.Max(Math.Max(width, height), 1.0));
        return new SignatureFrame(left, top, width, height, scale);
    }
}
