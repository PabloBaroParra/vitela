using Microsoft.UI;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;

namespace Pdf.Windows.Viewer;

/// <summary>
/// A white pad the reader signs on with the mouse, a pen or a finger — all of
/// them arrive as pointer events. It only collects strokes in pad pixels; the
/// PNG is made from them by <see cref="SignaturePng"/>.
/// </summary>
/// <remarks>
/// White whatever the theme: ink is always black, as on paper. A press with no
/// line behind it (a bare click) is not kept, so <see cref="HasInk"/> means
/// there is something to turn into an image.
/// </remarks>
public sealed class SignaturePadView : Grid
{
    /// <summary>Pen width on the pad; the PNG keeps it, so the signature looks as drawn.</summary>
    public const double StrokeWidth = 3;

    private readonly Canvas _canvas = new() { Background = new SolidColorBrush(Colors.White), Height = 200, ManipulationMode = ManipulationModes.None };
    private readonly List<List<PadPoint>> _strokes = [];
    private List<PadPoint>? _current;
    private Polyline? _currentLine;
    private uint? _pointerId;

    public SignaturePadView()
    {
        Children.Add(new Border
        {
            Child = _canvas,
            BorderThickness = new Thickness(1),
            BorderBrush = (Brush)Application.Current.Resources["ControlStrokeColorDefaultBrush"],
            CornerRadius = new CornerRadius(4),
        });
        AutomationProperties.SetName(this, "Signature pad");
        AutomationProperties.SetAutomationId(this, "SignaturePad");
        _canvas.SizeChanged += (_, _) => _canvas.Clip = new RectangleGeometry { Rect = new global::Windows.Foundation.Rect(0, 0, _canvas.ActualWidth, _canvas.ActualHeight) };
        _canvas.PointerPressed += OnPressed;
        _canvas.PointerMoved += OnMoved;
        _canvas.PointerReleased += (_, args) => Finish(args, keep: true);
        _canvas.PointerCaptureLost += (_, args) => Finish(args, keep: true);
        _canvas.PointerCanceled += (_, args) => Finish(args, keep: false);
    }

    /// <summary>Raised after a stroke is added or the pad is cleared.</summary>
    public event EventHandler? InkChanged;

    public bool HasContent => _strokes.Count > 0;

    public bool HasInk => DrawnSignature.HasInk(_strokes);

    /// <summary>A snapshot, so the dialog can be closed while the PNG is made.</summary>
    public IReadOnlyList<IReadOnlyList<PadPoint>> Strokes => [.. _strokes.Select(stroke => (IReadOnlyList<PadPoint>)[.. stroke])];

    public void Clear()
    {
        _strokes.Clear();
        _canvas.Children.Clear();
        _current = null;
        _currentLine = null;
        InkChanged?.Invoke(this, EventArgs.Empty);
    }

    private void OnPressed(object sender, PointerRoutedEventArgs args)
    {
        if (_pointerId is not null) return;
        var point = args.GetCurrentPoint(_canvas);
        // A mouse draws with the left button; touch and pen report contact as the same flag.
        if (point.PointerDeviceType == PointerDeviceType.Mouse && !point.Properties.IsLeftButtonPressed) return;
        if (!_canvas.CapturePointer(args.Pointer)) return;
        _pointerId = args.Pointer.PointerId;
        _current = [];
        _currentLine = new Polyline
        {
            Stroke = new SolidColorBrush(Colors.Black),
            StrokeThickness = StrokeWidth,
            StrokeLineJoin = PenLineJoin.Round,
            StrokeStartLineCap = PenLineCap.Round,
            StrokeEndLineCap = PenLineCap.Round,
        };
        _canvas.Children.Add(_currentLine);
        Append(point.Position);
        args.Handled = true;
    }

    private void OnMoved(object sender, PointerRoutedEventArgs args)
    {
        if (_pointerId != args.Pointer.PointerId || _current is null) return;
        Append(args.GetCurrentPoint(_canvas).Position);
        args.Handled = true;
    }

    private void Append(global::Windows.Foundation.Point position)
    {
        // Kept inside the pad: a drag that leaves it keeps its capture but not its ink.
        var x = Math.Clamp(position.X, 0, _canvas.ActualWidth);
        var y = Math.Clamp(position.Y, 0, _canvas.ActualHeight);
        _current!.Add(new PadPoint(x, y));
        _currentLine!.Points.Add(new global::Windows.Foundation.Point(x, y));
    }

    private void Finish(PointerRoutedEventArgs args, bool keep)
    {
        if (_pointerId != args.Pointer.PointerId || _current is null) return;
        var stroke = _current;
        var line = _currentLine!;
        _current = null;
        _currentLine = null;
        _pointerId = null;
        if (keep && stroke.Count >= 2) _strokes.Add(stroke);
        else _canvas.Children.Remove(line);
        _canvas.ReleasePointerCapture(args.Pointer);
        InkChanged?.Invoke(this, EventArgs.Empty);
    }
}
