using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// Organize pages: a grid of thumbnails standing in for the viewer, where a
/// page is dragged to a new position, turned a quarter, removed, or a blank
/// page is appended. Each is one undoable edit through
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
    /// <summary>Thumbnail box edge, in DIPs. Square, so every card is the same size whatever the page shape.</summary>
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
    private int _organizeDragFrom = -1;

    private sealed class OrganizeCard(Image thumbnail, TextBlock number)
    {
        public Image Thumbnail { get; } = thumbnail;
        public TextBlock Number { get; } = number;
    }

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

        // An open text editor has writes in flight against a page position
        // the grid is about to let the reader change. Land them first, then
        // drop the mode: its parsed runs are keyed by position too.
        await SettleContentEditorForHistoryAsync();
        if (_session is null) return;
        ResetContentEditMode();
        StopPlacingTextField();
        _armedAnnotation = null;
        _selectedAnnotationId = null;

        _organizing = true;
        _viewerStale = false;
        OrganizeButton.IsChecked = true;
        OrganizeGrid.ItemsSource = _organizeCards;
        PageScroller.Visibility = Visibility.Collapsed;
        OrganizePanel.Visibility = Visibility.Visible;
        AnnotationStatus.Text = "Drag a page to move it. Changes are one undo step each.";
        BuildOrganizeCards();
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

        UpdateAnnotationControls(_annotationState);
    }

    /// <summary>Hides the grid and forgets its cards, without touching the viewer.</summary>
    private void CloseOrganizeView()
    {
        _organizing = false;
        _thumbnailGeneration++;
        OrganizeButton.IsChecked = false;
        OrganizePanel.Visibility = Visibility.Collapsed;
        _organizeCards.Clear();
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
        _thumbnailGeneration++;
        _organizeCards.Clear();
        if (_session is null) return;

        for (var index = 0; index < _session.PageCount; index++)
        {
            _organizeCards.Add(CreateOrganizeCard(index));
        }

        _ = RenderThumbnailsAsync(_thumbnailGeneration);
    }

    private async void InsertBlankPageButton_Click(object sender, RoutedEventArgs e)
    {
        if (!_organizing || _session is null) return;
        var index = _session.PageCount;
        await EditPagesAsync(new PageEdit.InsertBlank(index), "Blank page added.", onSuccess: _ =>
            _organizeCards.Add(CreateOrganizeCard((int)index)));
    }

    private Border CreateOrganizeCard(int index)
    {
        var thumbnail = new Image { Stretch = Stretch.Uniform };
        var frame = new Border
        {
            Width = ThumbnailBox,
            Height = ThumbnailBox,
            Child = thumbnail,
        };
        var number = new TextBlock { Text = (index + 1).ToString(), VerticalAlignment = VerticalAlignment.Center };

        var rotateLeft = CardButton("", "Rotate left", mirrored: true);
        var rotateRight = CardButton("", "Rotate right", mirrored: false);
        var delete = CardButton("", "Delete page", mirrored: false);
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 2, HorizontalAlignment = HorizontalAlignment.Right };
        actions.Children.Add(rotateLeft);
        actions.Children.Add(rotateRight);
        actions.Children.Add(delete);

        var footer = new Grid();
        footer.Children.Add(number);
        footer.Children.Add(actions);

        var body = new StackPanel { Spacing = 6 };
        body.Children.Add(frame);
        body.Children.Add(footer);

        var card = new Border { Padding = new Thickness(8), Child = body };
        card.Tag = new OrganizeCard(thumbnail, number);
        rotateLeft.Click += async (_, _) => await EditPageAtCardAsync(card, index => new PageEdit.Rotate(index, -90), "Page rotated.");
        rotateRight.Click += async (_, _) => await EditPageAtCardAsync(card, index => new PageEdit.Rotate(index, 90), "Page rotated.");
        delete.Click += async (_, _) => await EditPageAtCardAsync(card, index => new PageEdit.Remove(index), "Page deleted.");
        return card;
    }

    private static Button CardButton(string glyph, string label, bool mirrored)
    {
        var icon = new FontIcon { Glyph = glyph, FontSize = 14 };
        if (mirrored)
        {
            icon.RenderTransformOrigin = new global::Windows.Foundation.Point(0.5, 0.5);
            icon.RenderTransform = new ScaleTransform { ScaleX = -1 };
        }

        var button = new Button { Content = icon, Padding = new Thickness(6) };
        AutomationProperties.SetName(button, label);
        ToolTipService.SetToolTip(button, label);
        return button;
    }

    private void OrganizeGrid_DragItemsStarting(object sender, DragItemsStartingEventArgs e)
    {
        _organizeDragFrom = e.Items.FirstOrDefault() is Border card ? _organizeCards.IndexOf(card) : -1;
    }

    /// <summary>
    /// The grid has already moved the card by the time this runs, so the
    /// card's new index is exactly the position the page should land at —
    /// the same meaning the core gives a move's target.
    /// </summary>
    private async void OrganizeGrid_DragItemsCompleted(ListViewBase sender, DragItemsCompletedEventArgs args)
    {
        var from = _organizeDragFrom;
        _organizeDragFrom = -1;
        if (args.DropResult != global::Windows.ApplicationModel.DataTransfer.DataPackageOperation.Move
            || from < 0
            || args.Items.FirstOrDefault() is not Border card)
        {
            return;
        }

        var to = _organizeCards.IndexOf(card);
        if (to < 0 || to == from) return;

        await EditPagesAsync(new PageEdit.Move((uint)from, (uint)to), "Page moved.", onSuccess: _ => RenumberOrganizeCards());
    }

    private async Task EditPageAtCardAsync(Border card, Func<uint, PageEdit> edit, string done)
    {
        var index = _organizeCards.IndexOf(card);
        if (index < 0) return;

        await EditPagesAsync(edit((uint)index), done, onSuccess: pageEdit =>
        {
            switch (pageEdit)
            {
                case PageEdit.Remove:
                    _organizeCards.RemoveAt(index);
                    RenumberOrganizeCards();
                    break;
                case PageEdit.Rotate:
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
        if (_session is null || _organizeBusy) return;

        _organizeBusy = true;
        OrganizeGrid.IsEnabled = false;
        InsertBlankPageButton.IsEnabled = false;
        // Before the edit, not after: the preview is rebuilt inside it, and a
        // thumbnail asked of the old layout could land once it has — on a
        // card that, after a drop, no longer sits where it was asked for.
        _thumbnailGeneration++;
        var result = await _facade.EditPagesAsync(_session.SessionId, edit);
        _organizeBusy = false;
        OrganizeGrid.IsEnabled = true;
        InsertBlankPageButton.IsEnabled = true;
        if (!_organizing) return;

        if (!result.IsSuccess)
        {
            AnnotationStatus.Text = result.Error!.Message;
            BuildOrganizeCards();
            return;
        }

        _session = result.Value!;
        _pagesEdited = true;
        _viewerStale = true;
        onSuccess(edit);
        // Picks up every card still blank, the turned one included.
        _ = RenderThumbnailsAsync(_thumbnailGeneration);
        AnnotationStatus.Text = $"{done} Changes are pending save.";
        RefreshSessionCommands();
        await RefreshAnnotationStateAsync();
    }

    private void RenumberOrganizeCards()
    {
        for (var index = 0; index < _organizeCards.Count; index++)
        {
            ((OrganizeCard)_organizeCards[index].Tag).Number.Text = (index + 1).ToString();
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
                card.Thumbnail.Source = await MaterializeBitmapAsync(result.Value!);
            }
        }
    }

    /// <summary>The DPI that fits a page's longer side into the thumbnail box at this display's scale.</summary>
    private static uint ThumbnailDpi(PageDimensions page, double rasterizationScale)
    {
        var longerSidePt = Math.Max(Math.Max(page.WidthPt, page.HeightPt), 1);
        return (uint)Math.Clamp(Math.Ceiling(ThumbnailBox * rasterizationScale * 72 / longerSidePt), 8, 300);
    }
}
