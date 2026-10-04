using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

public partial class App : Application
{
    private MainWindow? _window;
    public App() => BundledPdfium.PointCoreAtBundledLibrary();
    protected override async void OnLaunched(LaunchActivatedEventArgs args)
    {
        var output = Environment.GetEnvironmentVariable("VITELA_SMOKE_OUTPUT")
            ?? throw new InvalidOperationException("Set VITELA_SMOKE_OUTPUT to an existing directory.");
        _window = new MainWindow();
        _window.Activate();
        try
        {
            await _window.OrganizePagesSmokeAsync();
            File.WriteAllText(Path.Combine(output, "organize-pages-smoke.log"),
                "PASS page thumbnails/source labels; boundary keyboard actions; page move/rotate/delete one-step Undo/Redo; native-reorder completion; stale drag; busy gates; final-page deletion/empty Undo/insertion; native Documents edge-scroll driver/bounds/stop; sample restored. Physical mouse drag/GridView autoscroll not exercised.");
        }
        catch (Exception error) { File.WriteAllText(Path.Combine(output, "organize-pages-smoke.log"), "FAIL " + error); }
        finally { _window.Close(); }
    }
}

public sealed partial class MainWindow
{
    internal async Task OrganizePagesSmokeAsync()
    {
        static void Check(bool value, string message) { if (!value) throw new InvalidOperationException(message); }
        for (var attempt = 0; Content.XamlRoot is null && attempt < 100; attempt++) await Task.Delay(50);
        Check(Content.XamlRoot is not null, "Window did not load");
        await OpenDocumentAsync("Vitela sample.pdf", await File.ReadAllBytesAsync(SamplePath));
        await EnterOrganizeViewAsync();
        ShowOrganizeDocuments(false);
        var original = _session!.PageCount;
        async Task Ready()
        {
            await RefreshOrganizePageSourcesAsync(_thumbnailGeneration);
            for (var attempt = 0; !OrganizePagesReady && attempt < 100; attempt++) await Task.Delay(50);
            Check(OrganizePagesReady, "Pages never became ready");
        }
        await Ready();
        await RenderThumbnailsAsync(_thumbnailGeneration);
        Check(_organizeCards.Count == original && _organizeCards.All(card => ((OrganizeCard)card.Tag).Thumbnail.Source is not null),
            "Every sample page needs a thumbnail");
        Check(_organizeCards.All(card => ((OrganizeCard)card.Tag).Source.Visibility == Visibility.Collapsed), "Single source must be hidden");
        Check(!((OrganizeCard)_organizeCards[0].Tag).Previous.IsEnabled
            && !((OrganizeCard)_organizeCards[^1].Tag).Next.IsEnabled, "Boundary keyboard moves must be disabled");
        await InsertBlankPageAsync(original, PageOrientation.Portrait);
        Check(((OrganizeCard)_organizeCards[0].Tag).Source.Text == "Vitela sample.pdf"
            && ((OrganizeCard)_organizeCards[^1].Tag).Source.Text == "Blank page"
            && _organizeCards.All(card => ((OrganizeCard)card.Tag).Source.Visibility == Visibility.Visible), "Mixed source labels must follow core origins");
        var blank = _organizeCards[^1];
        await EditPageAtCardAsync(blank, DocumentBlockAction.Move, 0);
        Check(_organizeCards[0] == blank && ((OrganizeCard)blank.Tag).Number.Text == "1"
            && _organizePageSnapshot!.Blocks[0].Source == DocumentBlockSource.Blank, "Keyboard move must move page/card/provenance together");
        await RunOrganizeHistoryAsync(true);
        await Ready();
        Check(_organizePageSnapshot!.Blocks[^1].Source == DocumentBlockSource.Blank, "One Undo restores move");
        await RunOrganizeHistoryAsync(false);
        await Ready();
        Check(_organizePageSnapshot!.Blocks[0].Source == DocumentBlockSource.Blank, "Redo restores move");
        var page = _organizeCards[0];
        var dimensions = _session!.Pages[0];
        await EditPageAtCardAsync(page, DocumentBlockAction.RotateRight);
        Check(_session!.Pages[0].Rotation == PageRotation.Clockwise90, "Right turn must target the selected page");
        await RunOrganizeHistoryAsync(true);
        await Ready();
        Check(_session!.Pages[0] == dimensions, "One Undo restores turn");
        await EditPageAtCardAsync(_organizeCards[0], DocumentBlockAction.RotateLeft);
        Check(_session!.Pages[0].Rotation == PageRotation.Clockwise270, "Left turn must target the selected page");
        await RunOrganizeHistoryAsync(true);
        await Ready();
        var moved = _organizeCards[0];
        var drag = new OrganizePageDrag(_organizePageSnapshot!, moved, 0, _thumbnailGeneration);
        _organizeCards.Move(0, (int)original); // Native GridView applies this before DragItemsCompleted.
        await CompleteOrganizePageMoveAsync(drag, (int)original);
        Check(_organizeCards[^1] == moved && _organizePageSnapshot!.Blocks[^1].Source == DocumentBlockSource.Blank,
            "Native reorder completion must update core once without moving the card twice");
        await RunOrganizeHistoryAsync(true);
        await Ready();
        var revision = _organizePageSnapshot!.Revision;
        await CompleteOrganizePageMoveAsync(drag, 1);
        Check(_organizePageSnapshot.Revision == revision, "Drag started before history must not change the new grid");
        SetBusy(true);
        Check(!OrganizeGrid.CanDragItems && _organizeCards.All(card => ((OrganizeCard)card.Tag).Actions.All(button => !button.IsEnabled)),
            "Busy must disable all page actions and drag");
        SetBusy(false);
        await EditPageAtCardAsync(_organizeCards[0], DocumentBlockAction.Delete);
        Check(_session!.PageCount == original && _organizeCards.All(card => ((OrganizeCard)card.Tag).Source.Visibility == Visibility.Collapsed),
            "Deleting the last blank source must hide remaining provenance");
        await RunOrganizeHistoryAsync(true);
        await Ready();
        Check(_session!.PageCount == original + 1, "One Undo restores deleted page");
        await RunOrganizeHistoryAsync(true); // Undo move redo after intervening undone turns.
        await Ready();
        await RunOrganizeHistoryAsync(true); // Undo blank insertion.
        await Ready();
        Check(_session!.PageCount == original, "Insertion Undo must restore sample");
        for (var remaining = original; remaining > 0; remaining--)
        {
            await EditPageAtCardAsync(_organizeCards[0], DocumentBlockAction.Delete);
            Check(_session!.PageCount == remaining - 1 && _organizeCards.Count == remaining - 1, "Page deletion must reach zero");
        }
        Check(_organizeCards.Count == 0 && OrganizeUndoButton.IsEnabled, "Empty page grid must retain Undo");
        await InsertBlankPageAsync(0, PageOrientation.Portrait);
        Check(_organizeCards.Count == 1 && _session!.PageCount == 1, "Insertion must recover empty grid");
        await RunOrganizeHistoryAsync(true);
        for (var restored = 0u; restored < original; restored++) await RunOrganizeHistoryAsync(true);
        await Ready();
        Check(_session!.PageCount == original && _organizePageSnapshot!.Blocks.Count == 1
            && _organizePageSnapshot.Blocks[0].Source == DocumentBlockSource.Base, "Sample must be restored");
        ShowOrganizeDocuments(true);
        await PopulateOrganizeDocumentsAsync();
        // Isolated native scroll content: exercise the actual timer/ScrollViewer,
        // not a forged model or an import command unavailable through the FFI.
        OrganizeDocumentsList.Children.Clear();
        for (var index = 0; index < 30; index++) OrganizeDocumentsList.Children.Add(new Border { Height = 120 });
        for (var attempt = 0; OrganizeDocumentsScroller.ScrollableHeight == 0 && attempt < 100; attempt++) await Task.Delay(50);
        Check(OrganizeDocumentsScroller.ScrollableHeight > 0 && _organizeHeaderInitialized, "Scroll fixture did not lay out");
        OrganizeDocumentsScroller.ChangeView(null, 0, null, true);
        await Task.Delay(50);
        _organizeBlockDragToken = "native-scroll-smoke";
        _organizeScrollStep = 16;
        _organizeScrollTimer.Start();
        for (var attempt = 0; OrganizeDocumentsScroller.VerticalOffset == 0 && attempt < 100; attempt++) await Task.Delay(25);
        Check(OrganizeDocumentsScroller.VerticalOffset > 0, "Drag timer must move the native scroller");
        StopOrganizeScroll();
        await Task.Delay(50);
        var stopped = OrganizeDocumentsScroller.VerticalOffset;
        await Task.Delay(100);
        Check(OrganizeDocumentsScroller.VerticalOffset == stopped && !_organizeScrollTimer.IsEnabled, "Stopped drag must not keep scrolling");
        OrganizeDocumentsScroller.ChangeView(null, OrganizeDocumentsScroller.ScrollableHeight, null, true);
        await Task.Delay(100);
        _organizeScrollStep = 16;
        _organizeScrollTimer.Start();
        for (var attempt = 0; _organizeScrollTimer.IsEnabled && attempt < 100; attempt++) await Task.Delay(25);
        Check(!_organizeScrollTimer.IsEnabled && OrganizeDocumentsScroller.VerticalOffset <= OrganizeDocumentsScroller.ScrollableHeight,
            "Edge driver must stand down at the native scroll boundary");
        ShowOrganizeDocuments(false);
        await Ready();
        Check(_organizeBlockDragToken is null && !_organizeScrollTimer.IsEnabled, "View change must clear drag/scroll state");
    }
}
