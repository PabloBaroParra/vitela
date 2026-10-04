using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Windows.ApplicationModel.DataTransfer;

namespace Pdf.Windows;

/// <summary>Documents/Pages selection and revision-bound block cards, not core grouping rules.</summary>
public sealed partial class MainWindow
{
    private bool _showingOrganizeDocuments = true;
    private ulong _organizeDocumentsGeneration;
    private readonly List<(Button Button, bool Available)> _organizeDocumentActions = [];
    private const string BlockDragFormat = "Vitela.DocumentBlock";
    private string? _organizeBlockDragToken;

    private void OrganizeDocumentsToggle_Click(object sender, RoutedEventArgs e) => ShowOrganizeDocuments(true);
    private void OrganizePagesToggle_Click(object sender, RoutedEventArgs e) => ShowOrganizeDocuments(false);

    private void ShowOrganizeDocuments(bool showDocuments)
    {
        ClearOrganizePageState();
        _showingOrganizeDocuments = showDocuments;
        OrganizeDocumentsToggle.IsChecked = showDocuments;
        OrganizePagesToggle.IsChecked = !showDocuments;
        OrganizeDocumentsScroller.Visibility = showDocuments ? Visibility.Visible : Visibility.Collapsed;
        OrganizeGrid.Visibility = showDocuments ? Visibility.Collapsed : Visibility.Visible;
        OrganizeViewHint.Text = showDocuments ? "Drag a document to move all of its pages." : "Drag a page to reorder it.";
        AnnotationStatus.Text = OrganizeViewHint.Text;
        _thumbnailGeneration++;
        ClearOrganizeDocuments();
        _organizeCards.Clear();
        if (_organizing) BuildOrganizeCards();
    }

    private void ClearOrganizeDocuments()
    {
        StopOrganizeScroll();
        _organizeDocumentsGeneration++;
        _organizeBlockDragToken = null;
        OrganizeDocumentsList.Children.Clear();
        _organizeDocumentActions.Clear();
    }

    private void SetOrganizeDocumentActionsEnabled(bool ready)
    {
        foreach (var (button, available) in _organizeDocumentActions) button.IsEnabled = ready && available;
    }

    private async Task PopulateOrganizeDocumentsAsync()
    {
        ClearOrganizeDocuments();
        if (!_organizing || !_showingOrganizeDocuments || _session is not { } session) return;
        var generation = _organizeDocumentsGeneration;
        bool Current() => generation == _organizeDocumentsGeneration && _session?.SessionId == session.SessionId
            && _organizing && _showingOrganizeDocuments;
        var result = await _facade.DocumentBlocksAsync(session.SessionId);
        if (!Current()) return;
        if (!result.IsSuccess)
        {
            OrganizeDocumentsList.Children.Add(new TextBlock { Text = result.Error!.Message, TextWrapping = TextWrapping.Wrap });
            return;
        }
        var snapshot = result.Value!;
        var covers = new List<(Image Image, uint Page)>();
        OrganizeDocumentsList.Children.Add(CreateOrganizeBlockGap(snapshot, 0, session.SessionId, generation));
        for (var position = 0; position < snapshot.Blocks.Count; position++)
        {
            var block = snapshot.Blocks[position];
            var name = block.Source switch
            {
                DocumentBlockSource.Base => session.DisplayName,
                DocumentBlockSource.Blank => "Blank pages",
                _ => "Imported PDF",
            };
            if (block.Part is { } part) name += $" — Part {part}";
            var meta = block.Count == 1 ? $"1 page · {block.Start + 1}" : $"{block.Count} pages · {block.Start + 1}–{block.Start + block.Count}";
            var cover = new Image { Width = 96, Height = 124, Stretch = Stretch.Uniform };
            covers.Add((cover, block.Start));
            var sheets = new Grid { Width = 104, Height = 132 };
            for (var depth = 2; depth >= 1; depth--)
                if (block.Count > depth) sheets.Children.Add(new Border { Width = 96, Height = 124,
                    HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
                    Margin = new Thickness(depth * 4, depth * 4, 0, 0), Style = NamedStyle("OrganizeCoverSheetStyle") });
            // .organize-cover: the page render sits on the canvas colour
            sheets.Children.Add(new Border { Width = 96, Height = 124, Child = cover,
                HorizontalAlignment = HorizontalAlignment.Left, VerticalAlignment = VerticalAlignment.Top,
                Style = NamedStyle("OrganizeThumbStyle") });
            var details = new StackPanel { Spacing = 4, VerticalAlignment = VerticalAlignment.Center };
            details.Children.Add(new TextBlock { Text = name, Style = NamedStyle("OrganizeBlockNameStyle") });
            details.Children.Add(new TextBlock { Text = meta, Style = NamedStyle("OrganizeBlockMetaStyle") });
            var actions = new Viewer.ToolbarPanel { HorizontalAlignment = HorizontalAlignment.Left };
            var capturedPosition = position;
            void AddAction(string glyph, string label, DocumentBlockAction action, bool available, int slot = 0)
            {
                var button = CardButton(glyph, $"{label}: {name}", false);
                if (action == DocumentBlockAction.RotateLeft && button.Content is FontIcon icon)
                {
                    icon.RenderTransformOrigin = new global::Windows.Foundation.Point(0.5, 0.5);
                    icon.RenderTransform = new ScaleTransform { ScaleX = -1 };
                }
                AutomationProperties.SetAutomationId(button, $"OrganizeDocument_{position}_{label.Replace(" ", "")}");
                _organizeDocumentActions.Add((button, available));
                button.Click += async (_, _) => await EditOrganizeBlockAsync(snapshot, capturedPosition, action, slot, session.SessionId, generation);
                actions.Children.Add(button);
            }
            AddAction("\uE74A", "Move up", DocumentBlockAction.Move, position > 0, position - 1);
            AddAction("\uE74B", "Move down", DocumentBlockAction.Move, position + 1 < snapshot.Blocks.Count, position + 2);
            AddAction("\uE7AD", "Rotate left", DocumentBlockAction.RotateLeft, true);
            AddAction("\uE7AD", "Rotate right", DocumentBlockAction.RotateRight, true);
            AddAction("\uE74D", "Delete", DocumentBlockAction.Delete, true);
            // Controls wrap below the details in narrow windows; cover remains a native image.
            details.Children.Add(actions);
            var body = new Grid { ColumnSpacing = 12 };
            body.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
            body.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
            body.Children.Add(sheets);
            Grid.SetColumn(details, 1);
            body.Children.Add(details);
            var card = new Border { Child = body, CanDrag = true, Style = NamedStyle("OrganizeCardStyle") };
            AutomationProperties.SetName(card, $"{name}, {meta}, document {position + 1} of {snapshot.Blocks.Count}");
            AutomationProperties.SetAutomationId(card, $"OrganizeDocument_{position}");
            card.DragStarting += (_, args) =>
            {
                if (!Current() || _isBusy || _organizeBusy || _dialogOpen) { args.Cancel = true; return; }
                _organizeBlockDragToken = $"{session.SessionId}:{generation}:{capturedPosition}";
                args.Data.SetData(BlockDragFormat, _organizeBlockDragToken);
                args.Data.RequestedOperation = DataPackageOperation.Move;
            };
            card.DropCompleted += (_, _) => _organizeBlockDragToken = null;
            OrganizeDocumentsList.Children.Add(card);
            OrganizeDocumentsList.Children.Add(CreateOrganizeBlockGap(snapshot, position + 1, session.SessionId, generation));
        }
        UpdateOrganizeHeader();
        foreach (var (image, pageIndex) in covers)
        {
            if (!Current()) return;
            if (pageIndex >= session.Pages.Count) continue;
            var rendered = await _facade.RenderPageAsync(session.SessionId, pageIndex,
                ThumbnailDpi(session.Pages[(int)pageIndex], (_xamlRoot?.RasterizationScale ?? 1) * 124 / ThumbnailBox), false);
            if (!Current()) return;
            if (!rendered.IsSuccess) { ToolTipService.SetToolTip(image, rendered.Error!.Message); continue; }
            var bitmap = await MaterializeBitmapAsync(rendered.Value!);
            if (!Current()) return;
            image.Source = bitmap;
        }
    }

    private Border CreateOrganizeBlockGap(DocumentBlocksSnapshot snapshot, int slot, string sessionId, ulong generation)
    {
        var gap = new Border { AllowDrop = true, Style = NamedStyle("OrganizeGapStyle") };
        void Clear() => gap.Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        bool Ready(DragEventArgs args) => _session?.SessionId == sessionId && generation == _organizeDocumentsGeneration
            && !_isBusy && !_organizeBusy && !_dialogOpen && _organizeBlockDragToken is not null && args.DataView.Contains(BlockDragFormat);
        gap.DragOver += (_, args) =>
        {
            args.AcceptedOperation = Ready(args) ? DataPackageOperation.Move : DataPackageOperation.None;
            if (Ready(args)) gap.Background = ThemeBrush("VitelaAccentFillBrush");
            else Clear();
            args.Handled = true;
        };
        gap.DragLeave += (_, _) => Clear();
        gap.Drop += async (_, args) =>
        {
            var deferral = args.GetDeferral();
            try
            {
                Clear();
                if (!Ready(args)) return;
                var payload = await args.DataView.GetDataAsync(BlockDragFormat) as string;
                if (!Ready(args) || payload != _organizeBlockDragToken) return;
                var prefix = $"{sessionId}:{generation}:";
                if (payload is null || !payload.StartsWith(prefix, StringComparison.Ordinal)
                    || !int.TryParse(payload[prefix.Length..], out var position)) return;
                if (position < 0 || position >= snapshot.Blocks.Count || slot == position || slot == position + 1) return;
                await EditOrganizeBlockAsync(snapshot, position, DocumentBlockAction.Move, slot, sessionId, generation);
                args.Handled = true;
            }
            finally { Clear(); deferral.Complete(); }
        };
        return gap;
    }

    private async Task EditOrganizeBlockAsync(DocumentBlocksSnapshot snapshot, int position, DocumentBlockAction action, int slot,
        string sessionId, ulong generation)
    {
        if (_session?.SessionId != sessionId || generation != _organizeDocumentsGeneration || !_showingOrganizeDocuments) return;
        await EditPagesAsync(new PageEdit.Block(snapshot, position, action, slot),
            action == DocumentBlockAction.Delete ? "Document deleted." : action == DocumentBlockAction.Move ? "Document moved." : "Document rotated.", _ => { });
    }
}
