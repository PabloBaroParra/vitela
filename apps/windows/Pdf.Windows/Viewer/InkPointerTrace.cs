using Pdf.Windows.Facade;

namespace Pdf.Windows.Viewer;

/// <summary>Samples of one pointer gesture, without stationary move/release duplicates.</summary>
public sealed class InkPointerTrace
{
    private readonly List<AnnotationPoint> _points;

    public InkPointerTrace(AnnotationPoint origin) => _points = [origin];

    public IReadOnlyList<AnnotationPoint> Points => _points;

    public void Append(AnnotationPoint point)
    {
        // A press and release at the same position is still a single sample.
        // Keep returns to earlier positions: a closed stroke is a real drag.
        if (point != _points[^1]) _points.Add(point);
    }
}
