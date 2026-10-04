using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Organize pages: a grid of thumbnails standing in for the viewer, where a
/// page is dragged to a new position, turned a quarter, removed, or a blank
/// page is inserted. Each is one undoable edit through
/// <see cref="PdfDocumentFacade.EditPagesAsync"/>; the core owns what a move
/// means, this partial owns only the cards.
/// </summary>
/// <remarks>
/// A card is the page, not the position: it keeps its thumbnail through a
/// move, because the page it shows travels with it. Only its number is
/// positional, and every edit renumbers. The one card whose picture changes
/// is a turned one, and only that card is rendered again.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>Thumbnail rendering budget, in DIPs; fixed card frames contain either page orientation.</summary>
    private const double ThumbnailBox = 150;

    private readonly ObservableCollection<Border> _organizeCards = [];
    private bool _organizing;
    private bool _organizeBusy;
    /// <summary>
    /// Whether this session has ever moved, turned or removed a page. Latched,
    /// like <c>HasRecordedPreviewEdit</c> in the facade: from then on any undo
    /// or redo may change the page layout, so each one re-reads it.
    /// </summary>
    private bool _pagesEdited;
    /// <summary>Whether the hidden viewer still shows the pages as they were before organizing.</summary>
    private bool _viewerStale;
    /// <summary>Bumped on every rebuild and edit, so a thumbnail that finishes late is dropped rather than painted onto the wrong card.</summary>
    private ulong _thumbnailGeneration;

    private async void OrganizeButton_Click(object sender, RoutedEventArgs e)
    {
        if (_organizing)
        {
            LeaveOrganizeView();
        }
        else
        {
            await EnterOrganizeViewAsync();
        }
    }

    private async Task EnterOrganizeViewAsync()
    {
        if (_session is not { PageCount: > 0 })
        {
            OrganizeButton.IsChecked = false;
            return;
        }

        if (_isBusy || _organizeBusy || _dialogOpen) return;
        var sessionId = _session.SessionId;

        // An open text editor has writes in flight against a page position
        // the grid is about to let the reader change. Land them first, then
        // drop the mode: its parsed runs are keyed by position too.
        await SettleContentEditorForHistoryAsync();
        if (_session?.SessionId != sessionId || _isBusy) return;
        ResetContentEditMode();
        StopPlacingFormField();
        _armedAnnotation = null;
        _selectedAnnotationId = null;

        _organizing = true;
        _viewerStale = false;
        OrganizeButton.IsChecked = true;
        OrganizeGrid.ItemsSource = _organizeCards;
        PageScroller.Visibility = Visibility.Collapsed;
        OrganizePanel.Visibility = Visibility.Visible;
        EditorView.Visibility = Visibility.Collapsed;
        HomeView.Visibility = Visibility.Collapsed;
        MarkRailDestination("Organize");
        UpdateOrganizeHeader();
        UpdatePageNavigationControls();
        AnnotationStatus.Text = "Drag a page to move it. Changes are one undo step each.";
        ShowOrganizeDocuments(showDocuments: true);
        UpdateAnnotationControls(_annotationState);
    }

    private void LeaveOrganizeView()
    {
        if (!_organizing) return;

        CloseOrganizeView();
        if (_session is null) return;
        if (_viewerStale)
        {
            ShowReorganizedDocument(_session);
        }
        else
        {
            PageScroller.Visibility = Visibility.Visible;
        }

        SyncPageNavigation();
        UpdateAnnotationControls(_annotationState);
    }

    /// <summary>Hides the grid and forgets its cards, without touching the viewer.</summary>
    private void CloseOrganizeView()
    {
        _organizing = false;
        ClearOrganizePageState();
        _thumbnailGeneration++;
        OrganizeButton.IsChecked = false;
        if (OrganizePanel.Visibility == Visibility.Visible) EditorView.Visibility = Visibility.Visible;
        OrganizePanel.Visibility = Visibility.Collapsed;
        _organizeCards.Clear();
        ClearOrganizeDocuments();
        UpdateOrganizeHeader();
        UpdatePageNavigationControls();
    }

    /// <summary>
    /// Shows the same document with a new page layout. Everything the shell
    /// cached by page position — text for selection, search hits, parsed
    /// content — described the old layout, so it goes; annotations and fields
    /// are re-read, and the core reports them at their pages' new positions.
    /// </summary>
    private void ShowReorganizedDocument(DocumentSession session)
    {
        _viewerStale = false;
        _selectedAnnotationId = null;
        _pointerDrag = null;
        ResetSelectionState();
        ResetContentEditMode();
        ClearSearchResults();
        ShowDocumentPages(session);
        _ = RefreshAnnotationStateAsync();
        _ = RefreshFormFieldsAsync();
    }

    /// <summary>
    /// Re-reads the page layout after an undo or redo, which may have moved,
    /// turned or restored pages. A no-op in a session that never edited its
    /// pages: nothing else changes the layout.
    /// </summary>
    private async Task SyncPagesAfterHistoryAsync()
    {
        if (!_pagesEdited || _session is null) return;

        var result = await _facade.SessionAsync(_session.SessionId);
        if (!result.IsSuccess || _session?.SessionId != result.Value!.SessionId) return;
        _session = result.Value;
        RefreshSessionCommands();
        if (_organizing)
        {
            _viewerStale = true;
            BuildOrganizeCards();
        }
        else
        {
            ShowReorganizedDocument(_session);
        }
    }

    private void BuildOrganizeCards()
    {
        if (_showingOrganizeDocuments)
        {
            _ = PopulateOrganizeDocumentsAsync();
            return;
        }
        _thumbnailGeneration++;
        ClearOrganizePageState();
        _organizeCards.Clear();
        if (_session is null) return;

        for (var index = 0; index < _session.PageCount; index++)
        {
            _organizeCards.Add(CreateOrganizeCard(index));
        }

        _ = RenderThumbnailsAsync(_thumbnailGeneration);
        _ = RefreshOrganizePageSourcesAsync(_thumbnailGeneration);
    }

    private async void InsertBlankPageButton_Click(object sender, RoutedEventArgs e)
    {
        if (!_organizing || _session is null) return;
        await InsertBlankPageAsync(_session.PageCount, PageOrientation.Portrait);
    }

    private async void InsertLandscapePageButton_Click(object sender, RoutedEventArgs e)
    {
        if (!_organizing || _session is null) return;
        await InsertBlankPageAsync(_session.PageCount, PageOrientation.Landscape);
    }

    private async Task InsertBlankPageAsync(uint index, PageOrientation orientation)
    {
        await EditPagesAsync(new PageEdit.InsertBlank(index, orientation), "Blank page added.", onSuccess: _ =>
        {
            _organizeCards.Insert((int)index, CreateOrganizeCard((int)index));
            RenumberOrganizeCards();
        });
    }

    /// <summary>FOOTER_ICON_PX in the accent, as organize/grid/card.rs and documents/card.rs draw their actions.</summary>
    private static Button CardButton(ShellIcon icon, string label)
    {
        var button = new Button { Padding = new Thickness(6) };
        button.Content = ShellIconImage(icon, 16, IconTint.Accent, button);
        AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
        return button;
    }

    private void OrganizeGrid_DragItemsStarting(object sender, DragItemsStartingEventArgs e)
    {
        ClearOrganizePageDrag();
        if (!OrganizePagesReady || e.Items.Count != 1 || e.Items[0] is not Border card) { e.Cancel = true; return; }
        _organizePageDrag = new(_organizePageSnapshot!, card, _organizeCards.IndexOf(card), _thumbnailGeneration);
        e.Data.SetData(PageDragFormat, _organizePageDrag.Token);
        e.Data.RequestedOperation = global::Windows.ApplicationModel.DataTransfer.DataPackageOperation.Move;
    }

    /// <summary>
    /// The grid has already moved the card by the time this runs, so the
    /// card's new index is exactly the position the page should land at —
    /// the same meaning the core gives a move's target.
    /// </summary>
    private async void OrganizeGrid_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
    {
        var drag = _organizePageDrag;
        ClearOrganizePageDrag();
        if (args.DropResult != global::Windows.ApplicationModel.DataTransfer.DataPackageOperation.Move
            || drag is null || !OrganizePageDragCurrent(drag)
            || args.Items.FirstOrDefault() is not Border card || card != drag.Card)
        {
            return;
        }

        var to = _organizeCards.IndexOf(card);
        if (to < 0 || to == drag.From) return;

        await CompleteOrganizePageMoveAsync(drag, to);
    }

    private async Task CompleteOrganizePageMoveAsync(OrganizePageDrag drag, int to)
    {
        if (!OrganizePageDragCurrent(drag) || to < 0 || to >= _organizeCards.Count || to == drag.From) return;
        await EditPagesAsync(new PageEdit.OrganizePage(drag.Snapshot, (uint)drag.From, DocumentBlockAction.Move, (uint)to),
            "Page moved.", onSuccess: _ => RenumberOrganizeCards());
    }

    private async Task EditPageAtCardAsync(Border card, DocumentBlockAction action, int target = -1)
    {
        if (!OrganizePagesReady) return;
        var index = _organizeCards.IndexOf(card);
        if (index < 0) return;

        await EditPagesAsync(new PageEdit.OrganizePage(_organizePageSnapshot!, (uint)index, action, (uint)Math.Max(0, target)),
            action == DocumentBlockAction.Delete ? "Page deleted." : action == DocumentBlockAction.Move ? "Page moved." : "Page rotated.", onSuccess: _ =>
        {
            switch (action)
            {
                case DocumentBlockAction.Delete:
                    _organizeCards.RemoveAt(index);
                    RenumberOrganizeCards();
                    break;
                case DocumentBlockAction.Move:
                    _organizeCards.Move(index, target);
                    RenumberOrganizeCards();
                    break;
                case DocumentBlockAction.RotateLeft:
                case DocumentBlockAction.RotateRight:
                    // Only this page looks different now. Clearing the old
                    // picture keeps a stale upright page from standing in
                    // for the turned one while it renders.
                    ((OrganizeCard)card.Tag).Thumbnail.Source = null;
                    break;
            }
        });
    }

    /// <summary>
    /// Runs one page edit with the grid locked, so a second drag cannot start
    /// against a layout the core has not confirmed yet.
    /// </summary>
    /// <remarks>
    /// On failure the grid is rebuilt from the session rather than patched: a
    /// refused move has already been drawn by the grid itself, and the
    /// session is the only record of where the pages really are.
    /// </remarks>
    private async Task EditPagesAsync(PageEdit edit, string done, Action<PageEdit> onSuccess)
    {
        if (_session is null || _organizeBusy || _isBusy || _dialogOpen) return;

        _organizeBusy = true;
        SetBusy(true);
        UpdateOrganizeHeader();
        // Before the edit, not after: the preview is rebuilt inside it, and a
        // thumbnail asked of the old layout could land once it has — on a
        // card that, after a drop, no longer sits where it was asked for.
        _thumbnailGeneration++;
        ClearOrganizePageDrag();
        try
        {
            var result = await _facade.EditPagesAsync(_session.SessionId, edit);
            if (!_organizing) return;

            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                var current = await _facade.SessionAsync(_session.SessionId);
                if (current.IsSuccess) _session = current.Value!;
                // A preview failure can follow a recorded edit. Reconcile from
                // core and retain the history refresh obligation even on error.
                _pagesEdited = true;
                _viewerStale = true;
                RefreshSessionCommands();
                BuildOrganizeCards();
                await RefreshAnnotationStateAsync();
                return;
            }

            _session = result.Value!;
            _pagesEdited = true;
            _viewerStale = true;
            if (_showingOrganizeDocuments) await PopulateOrganizeDocumentsAsync();
            else
            {
                onSuccess(edit);
                await RefreshOrganizePageSourcesAsync(_thumbnailGeneration);
            }
            // Picks up every card still blank, the turned one included.
            _ = RenderThumbnailsAsync(_thumbnailGeneration);
            AnnotationStatus.Text = $"{done} Changes are pending save.";
            RefreshSessionCommands();
            await RefreshAnnotationStateAsync();
        }
        finally
        {
            _organizeBusy = false;
            if (!_windowClosed)
            {
                SetBusy(false);
                UpdateAnnotationControls(_annotationState);
                UpdateOrganizeHeader();
            }
        }
    }

    private void RenumberOrganizeCards()
    {
        for (var index = 0; index < _organizeCards.Count; index++)
        {
            ((OrganizeCard)_organizeCards[index].Tag).Number.Text = (index + 1).ToString();
            AutomationProperties.SetName(_organizeCards[index], $"Page {index + 1}");
        }
    }

    /// <summary>
    /// Fills in every card still missing its picture, one page at a time.
    /// Sequential on purpose: renders queue behind one another in the core
    /// anyway, and asking in page order paints the grid top to bottom.
    /// </summary>
    private async Task RenderThumbnailsAsync(ulong generation)
    {
        if (_session is null) return;
        var sessionId = _session.SessionId;
        var scale = _xamlRoot?.RasterizationScale ?? 1.0;

        for (var index = 0; index < _organizeCards.Count; index++)
        {
            if (generation != _thumbnailGeneration || _session?.SessionId != sessionId) return;
            var card = (OrganizeCard)_organizeCards[index].Tag;
            if (card.Thumbnail.Source is not null || index >= _session.Pages.Count) continue;

            var page = _session.Pages[index];
            var result = await _facade.RenderPageAsync(sessionId, (uint)index, ThumbnailDpi(page, scale), invertContentColors: false);
            if (generation != _thumbnailGeneration) return;
            if (result.IsSuccess)
            {
                var bitmap = await MaterializeBitmapAsync(result.Value!);
                if (generation != _thumbnailGeneration || _session?.SessionId != sessionId) return;
                card.Thumbnail.Source = bitmap;
            }
            else ToolTipService.SetToolTip(card.Thumbnail, result.Error!.Message);
        }
    }

    /// <summary>The DPI that fits a page's longer side into the thumbnail box at this display's scale.</summary>
    private static uint ThumbnailDpi(PageDimensions page, double rasterizationScale)
    {
        var longerSidePt = Math.Max(Math.Max(page.WidthPt, page.HeightPt), 1);
        return (uint)Math.Clamp(Math.Ceiling(ThumbnailBox * rasterizationScale * 72 / longerSidePt), 8, 300);
    }
}
