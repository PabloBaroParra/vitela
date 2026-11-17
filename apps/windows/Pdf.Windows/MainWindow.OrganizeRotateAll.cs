using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// Rotate all pages: every page a quarter turn either way, as one undo step.
/// </summary>
/// <remarks>
/// The GTK shell offers it from its command palette (#585), behind exactly
/// what opening Organize requires (#586). This shell has no palette, so it
/// sits in the Organize header — reachable only once Organize is open, which
/// is that same gate — and the core's own page-turn permission still answers
/// for the document, as it does for every other turn.
/// </remarks>
public sealed partial class MainWindow
{
    private const int QuarterTurnDegrees = 90;

    private async void RotateAllLeftButton_Click(object sender, RoutedEventArgs e) =>
        await RotateAllPagesAsync(-QuarterTurnDegrees);

    private async void RotateAllRightButton_Click(object sender, RoutedEventArgs e) =>
        await RotateAllPagesAsync(QuarterTurnDegrees);

    private async Task RotateAllPagesAsync(int deltaDegrees)
    {
        if (!_organizing || _session is not { PageCount: > 0 } session) return;
        var count = session.PageCount;
        var pages = count == 1 ? "1 page" : $"{count} pages";
        var direction = deltaDegrees > 0 ? "right" : "left";
        await EditPagesAsync(new PageEdit.RotateAll(deltaDegrees), $"Rotated {pages} {direction}.", onSuccess: _ =>
        {
            // Every page looks different now. Clearing the old pictures keeps
            // a stale upright page from standing in for a turned one while
            // the thumbnails render again.
            foreach (var card in _organizeCards) ((OrganizeCard)card.Tag).Thumbnail.Source = null;
        });
    }
}
