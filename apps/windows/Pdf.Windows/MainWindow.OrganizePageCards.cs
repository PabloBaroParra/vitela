using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.ApplicationModel.DataTransfer;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>Page-card presentation and native drag identity. Core retains page order and provenance.</summary>
public sealed partial class MainWindow
{
    private const string PageDragFormat = "Vitela.OrganizePage";
    private DocumentBlocksSnapshot? _organizePageSnapshot;
    private OrganizePageDrag? _organizePageDrag;
    private sealed record OrganizePageDrag(DocumentBlocksSnapshot Snapshot, Border Card, int From, ulong Generation)
    {
        public string Token { get; } = Guid.NewGuid().ToString("N");
    }

    private sealed class OrganizeCard(Image thumbnail, TextBlock number, TextBlock source, Button[] actions,
        MenuFlyoutItem previous, MenuFlyoutItem next)
    {
        public Image Thumbnail { get; } = thumbnail;
        public TextBlock Number { get; } = number;
        public TextBlock Source { get; } = source;
        public Button[] Actions { get; } = actions;
        public MenuFlyoutItem Previous { get; } = previous;
        public MenuFlyoutItem Next { get; } = next;
    }

    private bool OrganizePagesReady => _organizing && !_showingOrganizeDocuments && !_isBusy && !_organizeBusy && !_dialogOpen
        && _session is not null && _organizePageSnapshot?.SessionId == _session.SessionId;

    private void ClearOrganizePageDrag() => _organizePageDrag = null;
    private void ClearOrganizePageState()
    {
        _organizePageSnapshot = null;
        ClearOrganizePageDrag();
    }

    private bool OrganizePageDragCurrent(OrganizePageDrag drag) => OrganizePagesReady
        && drag.Generation == _thumbnailGeneration && drag.Snapshot == _organizePageSnapshot
        && drag.From >= 0 && _organizeCards.Contains(drag.Card);

    private void OrganizeGrid_DragOver(object sender, DragEventArgs args)
    {
        if (_organizePageDrag is { } drag && OrganizePageDragCurrent(drag) && args.DataView.Contains(PageDragFormat)) return;
        // Do not let an external file/text payload mutate the native ItemsSource.
        args.AcceptedOperation = DataPackageOperation.None;
        args.Handled = true;
    }

    private async Task RefreshOrganizePageSourcesAsync(ulong generation)
    {
        if (_session is not { } session || _showingOrganizeDocuments) return;
        var result = await _facade.DocumentBlocksAsync(session.SessionId);
        if (generation != _thumbnailGeneration || _session?.SessionId != session.SessionId || _showingOrganizeDocuments || !_organizing) return;
        if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
        _organizePageSnapshot = result.Value!;
        var origins = _organizePageSnapshot.Blocks.Select(block => (block.Source, block.ImportedSourceId)).Distinct().Count();
        foreach (var block in _organizePageSnapshot.Blocks)
        {
            var name = block.Source switch { DocumentBlockSource.Base => session.DisplayName,
                DocumentBlockSource.Blank => "Blank page", _ => ImportedBlockName(session.SessionId, block.ImportedSourceId) };
            for (var page = block.Start; page < block.Start + block.Count && page < _organizeCards.Count; page++)
            {
                var source = ((OrganizeCard)_organizeCards[(int)page].Tag).Source;
                source.Text = origins > 1 ? name : "";
                source.Visibility = origins > 1 ? Visibility.Visible : Visibility.Collapsed;
                ToolTipService.SetToolTip(source, origins > 1 ? name : null);
            }
        }
        UpdateOrganizePageActions();
    }

    private void UpdateOrganizePageActions()
    {
        for (var index = 0; index < _organizeCards.Count; index++)
        {
            var state = (OrganizeCard)_organizeCards[index].Tag;
            foreach (var button in state.Actions) button.IsEnabled = OrganizePagesReady;
            state.Previous.IsEnabled = OrganizePagesReady && index > 0;
            state.Next.IsEnabled = OrganizePagesReady && index + 1 < _organizeCards.Count;
        }
        OrganizeGrid.CanDragItems = OrganizePagesReady;
        OrganizeGrid.CanReorderItems = OrganizePagesReady;
    }

    private Border CreateOrganizeCard(int index)
    {
        var thumbnail = new Image { Stretch = Stretch.Uniform };
        var number = new TextBlock { Text = (index + 1).ToString(), VerticalAlignment = VerticalAlignment.Center, Style = NamedStyle("OrganizeBlockNameStyle") };
        var source = new TextBlock { Visibility = Visibility.Collapsed, Style = NamedStyle("OrganizeCardSourceStyle") };
        var left = CardButton(ShellIcon.RotateLeft, "Rotate page left");
        var right = CardButton(ShellIcon.RotateRight, "Rotate page right");
        var delete = CardButton(ShellIcon.Delete, "Delete page");
        var actions = new StackPanel { Orientation = Orientation.Horizontal, Spacing = 2 };
        actions.Children.Add(left);
        actions.Children.Add(right);
        actions.Children.Add(delete);
        var footer = new Grid();
        footer.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        footer.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        footer.Children.Add(number);
        Grid.SetColumn(actions, 1);
        footer.Children.Add(actions);
        var menu = new MenuFlyout();
        var previous = new MenuFlyoutItem { Text = "Move page earlier" };
        var next = new MenuFlyoutItem { Text = "Move page later" };
        var portrait = new MenuFlyoutItem { Text = "Insert portrait A4 before" };
        var landscape = new MenuFlyoutItem { Text = "Insert landscape A4 before" };
        menu.Items.Add(previous);
        menu.Items.Add(next);
        menu.Items.Add(new MenuFlyoutSeparator());
        menu.Items.Add(portrait);
        menu.Items.Add(landscape);
        var more = new Button { Content = "Page actions", Flyout = menu, HorizontalAlignment = HorizontalAlignment.Stretch };
        var body = new StackPanel { Width = ThumbnailBox, Spacing = 6 };
        body.Children.Add(new Border { Width = ThumbnailBox, Height = 180, Child = thumbnail, Style = NamedStyle("OrganizeThumbStyle") });
        body.Children.Add(source);
        body.Children.Add(footer);
        body.Children.Add(more);
        var card = new Border { Child = body, Style = NamedStyle("OrganizeCardStyle") };
        card.Tag = new OrganizeCard(thumbnail, number, source, [left, right, delete, more], previous, next);
        AutomationProperties.SetName(card, $"Page {index + 1}");
        left.Click += async (_, _) => await EditPageAtCardAsync(card, DocumentBlockAction.RotateLeft);
        right.Click += async (_, _) => await EditPageAtCardAsync(card, DocumentBlockAction.RotateRight);
        delete.Click += async (_, _) => await EditPageAtCardAsync(card, DocumentBlockAction.Delete);
        previous.Click += async (_, _) => await EditPageAtCardAsync(card, DocumentBlockAction.Move, _organizeCards.IndexOf(card) - 1);
        next.Click += async (_, _) => await EditPageAtCardAsync(card, DocumentBlockAction.Move, _organizeCards.IndexOf(card) + 1);
        async Task Insert(PageOrientation orientation)
        {
            if (!OrganizePagesReady) return;
            var position = _organizeCards.IndexOf(card);
            if (position >= 0) await InsertBlankPageAsync((uint)position, orientation);
        }
        portrait.Click += async (_, _) => await Insert(PageOrientation.Portrait);
        landscape.Click += async (_, _) => await Insert(PageOrientation.Landscape);
        foreach (var button in ((OrganizeCard)card.Tag).Actions) button.IsEnabled = false;
        return card;
    }
}
