using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Shows a note's text in a tooltip while the pointer rests on it, so the page
/// itself says what a note holds instead of only the Read note dialog.
/// </summary>
public sealed partial class MainWindow
{
    private const double NoteTooltipMaxWidth = 360;

    /// <summary>
    /// Hangs the tooltip on the note's own overlay element, which
    /// <see cref="PlaceOverPage"/> has already positioned through the page
    /// transform — rotation included — so nothing is recomputed here. A no-op
    /// for anything but a note with text.
    /// </summary>
    private void AttachNoteTooltip(Rectangle shape, Annotation annotation)
    {
        if (NoteHover.TextFor(annotation) is not { } text) return;

        // A rectangle with no fill is invisible to hit-testing, so the note
        // would never receive the pointer. The press still bubbles to the page
        // canvas, which hit-tests by annotation data, so gestures are unchanged.
        shape.Fill = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        var tip = new ToolTip
        {
            Content = new TextBlock { Text = text, TextWrapping = TextWrapping.Wrap, MaxWidth = NoteTooltipMaxWidth },
        };
        // Whether it may show is decided when it opens, not when it was drawn:
        // a dialog or a busy phase can begin while the overlay stays as it was.
        tip.Opened += (sender, _) =>
        {
            if (!NoteTooltipAllowed()) ((ToolTip)sender).IsOpen = false;
        };
        ToolTipService.SetToolTip(shape, tip);
    }

    private bool NoteTooltipAllowed() => NoteHover.MayShow(_organizing, _isBusy, _dialogOpen, _pointerDrag is not null);
}
