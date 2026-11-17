using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Localization;

namespace Pdf.Windows;

/// <summary>Annotation-page presentation only; MainWindow.Annotations owns modes and edits.</summary>
public sealed partial class MainWindow
{
    private void BuildAnnotationPanel(Viewer.ToolbarPanel row)
    {
        var primary = new UIElement[]
        {
            HighlightButton, UnderlineButton, StrikeoutButton, InkButton, NoteButton,
            FreeTextButton, ShapeButton, StampButton, PreviousAnnotationButton, NudgeButton,
            GrowButton, AnnotationColorButton, DeleteAnnotationButton,
        };
        foreach (var control in primary) row.Children.Remove(control);
        var extras = row.Children.ToArray();
        row.Children.Clear();
        foreach (var control in primary) row.Children.Add(control);
        AnnotationColorButton.Content = I18n.T("annotate-restyle");
        AutomationProperties.SetName(AnnotationColorButton, I18n.T("annotate-restyle"));
        ToolTipService.SetToolTip(AnnotationColorButton, I18n.T("annotate-color-title"));
        AutomationProperties.SetName(row, I18n.T("tools-annotations"));
        var page = _toolPages["Annotate"];
        page.Children.Insert(0, new TextBlock
        {
            Text = I18n.T("tools-annotations"), FontSize = 18, FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        page.Children.Insert(1, row);
        // Preserve Windows' numeric move/resize, note reading and next-selection commands.
        var more = new Viewer.ToolbarPanel();
        foreach (var control in extras) more.Children.Add(control);
        page.Children.Insert(2, more);
    }
}
