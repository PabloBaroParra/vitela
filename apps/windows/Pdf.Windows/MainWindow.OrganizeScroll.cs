using Microsoft.UI.Xaml;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    // GridView already scrolls its own native reorder gesture. Documents uses a
    // plain ScrollViewer, so it needs an edge driver without becoming a drop target.
    private readonly DispatcherTimer _organizeScrollTimer = new() { Interval = TimeSpan.FromMilliseconds(16) };
    private double _organizeScrollStep;

    private void InitializeOrganizeScroll()
    {
        OrganizeDocumentsScroller.AllowDrop = true;
        OrganizeDocumentsScroller.AddHandler(UIElement.DragOverEvent, new DragEventHandler((_, args) =>
        {
            if (!_organizing || !_showingOrganizeDocuments || _isBusy || _organizeBusy || _dialogOpen
                || _organizeBlockDragToken is null || !args.DataView.Contains(BlockDragFormat)) { StopOrganizeScroll(); return; }
            _organizeScrollStep = DragEdgeScroll.Step(args.GetPosition(OrganizeDocumentsScroller).Y, OrganizeDocumentsScroller.ActualHeight);
            if (_organizeScrollStep == 0) StopOrganizeScroll();
            else _organizeScrollTimer.Start();
        }), true);
        OrganizeDocumentsScroller.AddHandler(UIElement.DragLeaveEvent, new DragEventHandler((_, _) => StopOrganizeScroll()), true);
        OrganizeDocumentsScroller.AddHandler(UIElement.DropEvent, new DragEventHandler((_, _) => StopOrganizeScroll()), true);
        _organizeScrollTimer.Tick += (_, _) =>
        {
            if (_windowClosed || !_organizing || !_showingOrganizeDocuments || _isBusy || _organizeBusy || _organizeBlockDragToken is null)
            { StopOrganizeScroll(); return; }
            var scroll = OrganizeDocumentsScroller;
            var target = Math.Clamp(scroll.VerticalOffset + _organizeScrollStep, 0, scroll.ScrollableHeight);
            if (Math.Abs(target - scroll.VerticalOffset) < 0.01) { StopOrganizeScroll(); return; }
            scroll.ChangeView(null, target, null, disableAnimation: true);
        };
    }

    private void StopOrganizeScroll()
    {
        _organizeScrollStep = 0;
        _organizeScrollTimer.Stop();
    }
}
