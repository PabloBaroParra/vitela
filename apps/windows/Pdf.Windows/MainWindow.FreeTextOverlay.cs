using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Paints a text box over its page. The text is the core's lines, one
/// <c>TextBlock</c> each — never a wrap computed here — laid out upright in the
/// box's own frame and turned with the page by <see cref="PlaceUpright"/>, the
/// same transform stamps use.
/// </summary>
public sealed partial class MainWindow
{
    private static readonly SolidColorBrush FreeTextInkBrush = new(Colors.Black);

    /// <param name="rect">
    /// Where to paint: the stored rect, or the one under the pointer during a
    /// move or resize. A different size means the lines no longer fit, so the
    /// core is asked to lay the text out again — still not a shell wrap.
    /// </param>
    private void DrawFreeText(PageSlot slot, uint pageIndex, Annotation annotation, AnnotationRect rect, bool selected)
    {
        var shown = annotation with { Rect = rect, Layout = LayoutFor(annotation, rect) };
        if (FreeTextDraw.Plan(shown, slot.Scale) is not { } plan) return;

        var box = new Canvas
        {
            Width = plan.WidthPx,
            Height = plan.HeightPx,
            Clip = new RectangleGeometry { Rect = new global::Windows.Foundation.Rect(0, 0, plan.WidthPx, plan.HeightPx) },
            IsHitTestVisible = false,
        };
        foreach (var line in plan.Lines)
        {
            var block = new TextBlock
            {
                Text = line.Text,
                FontFamily = new FontFamily("Arial"),
                FontSize = plan.FontSizePx,
                Foreground = FreeTextInkBrush,
                TextWrapping = TextWrapping.NoWrap,
                IsTextScaleFactorEnabled = false,
                IsHitTestVisible = false,
            };
            Canvas.SetLeft(block, line.Left);
            Canvas.SetTop(block, line.Top);
            box.Children.Add(block);
        }
        PlaceUpright(box, slot, (int)pageIndex, rect);
        slot.Annotations.Children.Add(box);

        if (selected)
        {
            // The box has no border in the file; this outline is only the editor showing its edge.
            var outline = new Rectangle { Stroke = HandleBrush, StrokeThickness = 1, StrokeDashArray = [4, 3], IsHitTestVisible = false };
            PlaceOverPage(outline, slot, (int)pageIndex, rect, minSide: 2);
            slot.Annotations.Children.Add(outline);
        }
    }

    private FreeTextLayout? LayoutFor(Annotation annotation, AnnotationRect rect)
    {
        if (annotation.Rect == rect) return annotation.Layout;
        var result = _facade.LayoutFreeText(annotation.Contents ?? string.Empty, rect.Width, rect.Height);
        return result.IsSuccess ? result.Value : annotation.Layout;
    }
}
