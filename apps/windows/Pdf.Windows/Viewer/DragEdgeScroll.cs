namespace Pdf.Windows.Viewer;

/// <summary>Presentation-only edge throttle; native ScrollViewer owns the scroll offset.</summary>
internal static class DragEdgeScroll
{
    public static double Step(double y, double height)
    {
        var edge = Math.Min(56, height / 3);
        if (edge <= 0) return 0;
        var depth = y < edge ? edge - y : y > height - edge ? y - (height - edge) : 0;
        if (depth == 0) return 0;
        var speed = 2 + 14 * Math.Clamp(depth / edge, 0, 1);
        return y < edge ? -speed : speed;
    }
}
