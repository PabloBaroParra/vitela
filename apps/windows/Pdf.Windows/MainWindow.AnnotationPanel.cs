using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;

namespace Pdf.Windows;

/// <summary>Annotation-page presentation only; MainWindow.Annotations owns modes and edits.</summary>
public sealed partial class MainWindow
{
    private void BuildAnnotationPanel(Viewer.ToolbarPanel row)
    {
        var primary = new UIElement[]
        {
            HighlightButton, UnderlineButton, StrikeoutButton, InkButton, NoteButton,
            ShapeButton, StampButton, PreviousAnnotationButton, NudgeButton,
            GrowButton, AnnotationColorButton, DeleteAnnotationButton,
        };
        foreach (var control in primary) row.Children.Remove(control);
        var extras = row.Children.ToArray();
        row.Children.Clear();
        foreach (var control in primary) row.Children.Add(control);
        AnnotationColorButton.Content = "Restyle";
        AutomationProperties.SetName(AnnotationColorButton, "Restyle");
        ToolTipService.SetToolTip(AnnotationColorButton, "Choose annotation color");
        AutomationProperties.SetName(row, "Annotations");
        var page = _toolPages["Annotate"];
        page.Children.Insert(0, new TextBlock
        {
            Text = "Annotations", FontSize = 18, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        page.Children.Insert(1, row);
        // Preserve Windows' numeric move/resize, note reading and next-selection commands.
        var more = new Viewer.ToolbarPanel();
        foreach (var control in extras) more.Children.Add(control);
        page.Children.Insert(2, more);
    }
}
