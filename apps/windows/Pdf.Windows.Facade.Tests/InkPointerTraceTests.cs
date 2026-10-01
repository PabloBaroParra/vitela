using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

internal static class InkPointerTraceTests
{
    public static Task RunAsync()
    {
        var origin = new AnnotationPoint(100, 200);
        var tap = new InkPointerTrace(origin);
        tap.Append(origin); // Release without any move event.
        AssertPoints(tap, [origin], "A tap must not become a two-point stroke.");
        tap.Append(origin); // Stationary move events must not change this.
        tap.Append(origin);
        AssertPoints(tap, [origin], "Stationary events must not create a stroke.");

        var horizontal = new AnnotationPoint(140, 200);
        var vertical = new AnnotationPoint(140, 240);
        var drag = new InkPointerTrace(origin);
        drag.Append(horizontal);
        drag.Append(horizontal);
        drag.Append(vertical);
        drag.Append(origin); // A closed stroke must not be mistaken for a tap.
        drag.Append(origin);
        AssertPoints(drag, [origin, horizontal, vertical, origin], "Keep the ordered path and its return to the origin.");

        var releaseOnly = new InkPointerTrace(origin);
        releaseOnly.Append(vertical);
        AssertPoints(releaseOnly, [origin, vertical], "A moved release must complete a stroke without move events.");
        return Task.CompletedTask;
    }

    private static void AssertPoints(InkPointerTrace trace, AnnotationPoint[] expected, string message)
    {
        if (!trace.Points.SequenceEqual(expected)) throw new InvalidOperationException(message);
    }
}
