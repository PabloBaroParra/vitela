using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Foundation;

namespace Pdf.Windows.Viewer;

/// <summary>Wraps toolbar controls or control groups without changing their tab order.</summary>
public sealed class ToolbarPanel : Panel
{
    /// <summary>Gap between items in a row. Toolbars keep 12; Home's recent cards use Linux's 10.</summary>
    public double ColumnSpacing { get; set; } = 12;

    /// <summary>Gap between rows. Toolbars keep 8; Home's recent cards use Linux's 10.</summary>
    public double RowSpacing { get; set; } = 8;

    protected override Size MeasureOverride(Size availableSize)
    {
        foreach (var child in Children)
            child.Measure(new Size(availableSize.Width, double.PositiveInfinity));
        return LayoutRows(availableSize.Width, arrange: false);
    }

    protected override Size ArrangeOverride(Size finalSize)
    {
        LayoutRows(finalSize.Width, arrange: true);
        return finalSize;
    }

    // Both passes use the same row breaks. Allocate the full row height to
    // each child so short labels can center beside taller buttons and inputs.
    private Size LayoutRows(double availableWidth, bool arrange)
    {
        var row = new List<UIElement>();
        double rowWidth = 0, rowHeight = 0, width = 0, top = 0;

        void FinishRow()
        {
            if (row.Count == 0) return;
            if (arrange)
            {
                double left = 0;
                foreach (var child in row)
                {
                    child.Arrange(new Rect(left, top, child.DesiredSize.Width, rowHeight));
                    left += child.DesiredSize.Width + ColumnSpacing;
                }
            }
            width = Math.Max(width, rowWidth);
            top += rowHeight + RowSpacing;
            row.Clear();
            rowWidth = rowHeight = 0;
        }

        foreach (var child in Children)
        {
            var size = child.DesiredSize;
            if (child.Visibility == Visibility.Collapsed || (size.Width == 0 && size.Height == 0)) continue;
            var gap = row.Count == 0 ? 0 : ColumnSpacing;
            if (row.Count > 0 && rowWidth + gap + size.Width > availableWidth)
            {
                FinishRow();
                gap = 0;
            }
            row.Add(child);
            rowWidth += gap + size.Width;
            rowHeight = Math.Max(rowHeight, size.Height);
        }
        FinishRow();
        return new Size(width, Math.Max(0, top - RowSpacing));
    }
}
