using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Printing;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using System.Runtime.InteropServices;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using Windows.Graphics.Printing;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow : Window
{
    private const uint RenderDpi = 144;
    private const uint PrintDpi = 300;
    private const double PointsToDips = RenderDpi / 72.0;
    private const double PageSpacing = 12;
    /// <summary>Rendered pages kept alive beyond the visible range, per side.</summary>
    private const int KeepWindow = 2;

    private readonly PdfDocumentFacade _facade = new(new GeneratedPdfCore(), new DebugDiagnosticLogger());
    private DocumentSession? _session;
    private List<PageSlot> _slots = [];
    private PrintManager? _printManager;
    private PrintDocument? _printDocument;
    // Captured on the UI thread: PrintTaskRequested fires on a print worker
    // thread, and reading PrintDocument.DocumentSource there throws
    // RPC_E_WRONG_THREAD, leaving the preview stuck on "loading".
    private IPrintDocumentSource? _printDocumentSource;
    private PrintJob? _printJob;
    private PrintTaskOptions? _printTaskOptions;

    public MainWindow()
    {
        InitializeComponent();
    }

    private async void OpenButton_Click(object sender, RoutedEventArgs e)
    {
        var picker = new FileOpenPicker();
        picker.FileTypeFilter.Add(".pdf");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        StorageFile? file = await picker.PickSingleFileAsync();
        if (file is null)
        {
            return;
        }

        SetBusy(true);
        byte[] bytes;
        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out bytes);
        }
        catch (Exception error)
        {
            SetBusy(false);
            ShowError(_facade.OpenReadFailure(error).Error!);
            return;
        }

        var result = await _facade.OpenAsync(new DocumentSource(file.Name, bytes));
        SetBusy(false);

        // An encrypted document surfaces as a typed password failure rather
        // than a dead-end error: prompt for the password and retry instead of
        // stranding the user on the generic error state.
        if (!result.IsSuccess && result.Error!.RequiresPassword)
        {
            var unlocked = await OpenWithPasswordAsync(file.Name, bytes);
            if (unlocked is null)
            {
                // The user dismissed the prompt; leave the current view as-is.
                return;
            }

            result = unlocked;
        }

        if (!result.IsSuccess)
        {
            ShowError(result.Error!);
            return;
        }

        ShowOpenedDocument(result.Value!);
    }

    private void ShowOpenedDocument(DocumentSession session)
    {
        _session = session;
        DocumentTitle.Text = _session.DisplayName;
        ClearSearchResults();
        if (_session.State == DocumentSessionState.Empty)
        {
            ShowEmpty("This document has no pages.");
            return;
        }

        BuildPagePlaceholders(_session);
        PageScroller.ChangeView(0, 0, 1, disableAnimation: true);
        PageScroller.Visibility = Visibility.Visible;
        EmptyState.Visibility = Visibility.Collapsed;
        ErrorState.Visibility = Visibility.Collapsed;
        PageScroller.UpdateLayout();
        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// Prompts for the document password and retries opening until it
    /// succeeds, the user cancels, or a non-password failure occurs. Returns
    /// the successful (or terminally-failed) result, or null if the user
    /// dismissed the prompt without unlocking the document.
    /// </summary>
    private async Task<OperationResult<DocumentSession>?> OpenWithPasswordAsync(string displayName, byte[] bytes)
    {
        var passwordBox = new PasswordBox { PlaceholderText = "Password" };
        var errorText = new TextBlock
        {
            Foreground = new SolidColorBrush(Microsoft.UI.Colors.Crimson),
            TextWrapping = TextWrapping.Wrap,
            Visibility = Visibility.Collapsed,
        };
        var panel = new StackPanel { Spacing = 8 };
        panel.Children.Add(new TextBlock
        {
            Text = "This document is password protected.",
            TextWrapping = TextWrapping.Wrap,
        });
        panel.Children.Add(passwordBox);
        panel.Children.Add(errorText);

        var dialog = new ContentDialog
        {
            Title = "Password required",
            Content = panel,
            PrimaryButtonText = "Open",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        while (true)
        {
            if (await dialog.ShowAsync() != ContentDialogResult.Primary)
            {
                return null;
            }

            var password = passwordBox.Password;
            SetBusy(true);
            var result = await _facade.OpenAsync(new DocumentSource(displayName, bytes), password);
            SetBusy(false);

            if (result.IsSuccess || !result.Error!.RequiresPassword)
            {
                // Either it opened, or it failed for a reason retrying cannot
                // fix — hand it back to the normal success/error handling.
                return result;
            }

            // Wrong password: keep the prompt open with a clear message.
            errorText.Text = "The password is incorrect. Try again.";
            errorText.Visibility = Visibility.Visible;
            passwordBox.Password = "";
        }
    }

    private void PageScroller_ViewChanged(object? sender, ScrollViewerViewChangedEventArgs e)
    {
        UpdateViewport(e.IsIntermediate);
    }

    private void PageScroller_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// One entry per page: a fixed-size placeholder sized from the page's
    /// point dimensions, filled in lazily as renders complete.
    /// </summary>
    private void BuildPagePlaceholders(DocumentSession session)
    {
        PageStack.Children.Clear();
        PageStack.Spacing = PageSpacing;
        _slots = new List<PageSlot>((int)session.PageCount);

        double top = 0;
        foreach (var page in session.Pages)
        {
            var image = new Image { Stretch = Stretch.Fill };
            var highlights = new Canvas { IsHitTestVisible = false };
            var pageLayer = new Grid();
            pageLayer.Children.Add(image);
            pageLayer.Children.Add(highlights);
            var container = new Border
            {
                Width = page.WidthPt * PointsToDips,
                Height = page.HeightPt * PointsToDips,
                BorderThickness = new Thickness(1),
                BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                // Unrendered pages read as paper, not as a hole in the theme.
                Background = new SolidColorBrush(Microsoft.UI.Colors.White),
                Child = pageLayer,
            };
            _slots.Add(new PageSlot(container, image, highlights, top, container.Height));
            top += container.Height + PageSpacing;
            PageStack.Children.Add(container);
        }
    }

    private void UpdateViewport(bool intermediate)
    {
        if (_session is null || _slots.Count == 0)
        {
            return;
        }

        var viewportTop = PageScroller.VerticalOffset;
        var viewportBottom = viewportTop + PageScroller.ViewportHeight;
        var firstVisible = _slots.FindLastIndex(slot => slot.Top <= viewportTop);
        if (firstVisible < 0 || _slots[firstVisible].Top + _slots[firstVisible].Height < viewportTop)
        {
            firstVisible = Math.Clamp(firstVisible + 1, 0, _slots.Count - 1);
        }

        var lastVisible = firstVisible;
        while (lastVisible + 1 < _slots.Count && _slots[lastVisible + 1].Top < viewportBottom)
        {
            lastVisible++;
        }

        PageCounter.Text = $"Page {firstVisible + 1} of {_slots.Count}";

        // Request renders even mid-scroll: the facade coalesces per-page
        // requests, and starting early is what makes pages arrive in time.
        var sessionId = _session.SessionId;
        for (var index = Math.Max(0, firstVisible - 1); index <= Math.Min(_slots.Count - 1, lastVisible + 1); index++)
        {
            var slot = _slots[index];
            if (!slot.Rendered && !slot.Requested)
            {
                slot.Requested = true;
                _ = RenderSlotAsync(sessionId, index);
            }
        }

        // Evict only once the scroll settles, so flinging past pages does
        // not churn bitmaps that are about to come back.
        if (intermediate)
        {
            return;
        }

        for (var index = 0; index < _slots.Count; index++)
        {
            if (index >= firstVisible - KeepWindow && index <= lastVisible + KeepWindow)
            {
                continue;
            }

            var slot = _slots[index];
            if (slot.Rendered && !slot.Requested)
            {
                slot.Image.Source = null;
                slot.Rendered = false;
            }
        }
    }

    private async Task RenderSlotAsync(string sessionId, int pageIndex)
    {
        var result = await _facade.RenderPageAsync(sessionId, (uint)pageIndex, RenderDpi, false);
        if (_session?.SessionId != sessionId || pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[pageIndex];
        slot.Requested = false;
        if (result.IsDiscarded || result.IsEmpty)
        {
            return;
        }

        if (!result.IsSuccess)
        {
            // A single failed page keeps its placeholder; only fail the whole
            // view when nothing has rendered at all (e.g. a broken document).
            if (!_slots.Any(other => other.Rendered))
            {
                ShowError(result.Error!);
            }

            return;
        }

        slot.Image.Source = await MaterializeBitmapAsync(result.Value!);
        slot.Rendered = true;
    }

    private async void SearchButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null)
        {
            return;
        }

        var query = SearchBox.Text.Trim();
        ClearSearchResults();
        if (query.Length == 0)
        {
            SearchStatus.Text = "Enter text to find.";
            return;
        }

        SearchButton.IsEnabled = false;
        var result = await _facade.SearchAsync(_session.SessionId, query);
        SearchButton.IsEnabled = true;
        if (result.IsDiscarded)
        {
            return;
        }

        if (!result.IsSuccess)
        {
            SearchStatus.Text = result.Error!.Message;
            return;
        }

        var search = result.Value!;
        SearchStatus.Text = search.Hits.Count == 0 ? "No matches." : $"{search.Hits.Count} match(es).";
        foreach (var hit in search.Hits)
        {
            SearchResultsList.Items.Add(new ListViewItem
            {
                Content = $"Page {hit.PageIndex + 1}: {hit.Text}",
                Tag = hit,
            });
        }
    }

    private async void PrintButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _session.State == DocumentSessionState.Empty)
        {
            PrintStatus.Text = "Open a document with pages before printing.";
            return;
        }

        var session = _session;

        // Pages are rendered lazily: the dialog opens immediately, preview
        // pages render on demand at screen DPI (GetPreviewPage), and the
        // full-quality pass at PrintDpi only runs once the user confirms
        // printing (AddPages).
        _printJob = new PrintJob(session.SessionId, session.DisplayName, session.Pages.Count);
        try
        {
            EnsurePrintingAvailable();
            await PrintManagerInterop.ShowPrintUIForWindowAsync(WindowNative.GetWindowHandle(this));
        }
        catch (Exception error) when (error is COMException or InvalidOperationException or NotImplementedException)
        {
            PrintStatus.Text = "Printing is unavailable on this Windows installation.";
            _printJob = null;
        }
        // NOTE: do NOT tear down _printJob/_printTaskOptions after ShowPrintUI.
        // ShowPrintUIForWindowAsync returns when the dialog is *shown*, not
        // when it is dismissed; the preview pipeline (Paginate/GetPreviewPage/
        // AddPages) runs afterwards while the dialog is open and needs this
        // state alive. It is released in PrintTask_Completed instead.
    }

    private void EnsurePrintingAvailable()
    {
        if (_printManager is not null)
        {
            return;
        }

        _printDocument = new PrintDocument();
        _printDocument.Paginate += PrintDocument_Paginate;
        _printDocument.GetPreviewPage += PrintDocument_GetPreviewPage;
        _printDocument.AddPages += PrintDocument_AddPages;
        _printDocumentSource = _printDocument.DocumentSource;
        _printManager = PrintManagerInterop.GetForWindow(WindowNative.GetWindowHandle(this));
        _printManager.PrintTaskRequested += PrintManager_PrintTaskRequested;
    }

    private void PrintManager_PrintTaskRequested(PrintManager sender, PrintTaskRequestedEventArgs args)
    {
        if (_printJob is null || _printDocumentSource is null)
        {
            return;
        }

        var source = _printDocumentSource;
        var task = args.Request.CreatePrintTask(_printJob.DisplayName, sourceArgs => sourceArgs.SetSource(source));
        task.Completed += PrintTask_Completed;
    }

    private void PrintTask_Completed(PrintTask sender, PrintTaskCompletedEventArgs args)
    {
        DispatcherQueue.TryEnqueue(() =>
        {
            _printJob = null;
            _printTaskOptions = null;
        });
    }

    private void PrintDocument_Paginate(object sender, PaginateEventArgs e)
    {
        _printTaskOptions = e.PrintTaskOptions;
        _printDocument!.SetPreviewPageCount(_printJob!.PageCount, PreviewPageCountType.Final);
    }

    private async void PrintDocument_GetPreviewPage(object sender, GetPreviewPageEventArgs e)
    {
        // The preview contract is collaborative: the pane shows its own
        // spinner until SetPreviewPage is called, which may happen after this
        // handler returns. Render lazily at screen DPI — the full-quality
        // pass only runs at print time in AddPages.
        var job = _printJob;
        if (job is null)
        {
            return;
        }

        try
        {
            var bitmap = await GetPreviewBitmapAsync(job, e.PageNumber - 1);
            if (bitmap is null || _printJob != job)
            {
                return;
            }

            _printDocument!.SetPreviewPage(e.PageNumber, CreatePrintPage(bitmap, e.PageNumber - 1, _printTaskOptions!));
        }
        catch (Exception)
        {
            // async void: an unhandled exception here crashes the process.
            // A failed page simply stays on the preview pane's spinner.
        }
    }

    private async void PrintDocument_AddPages(object sender, AddPagesEventArgs e)
    {
        var job = _printJob;
        if (job is null)
        {
            return;
        }

        try
        {
            for (var pageIndex = 0; pageIndex < job.PageCount; pageIndex++)
            {
                var bitmap = await RenderPrintBitmapAsync(job, pageIndex, PrintDpi);
                if (bitmap is null)
                {
                    PrintStatus.Text = "Unable to prepare all pages; the print job was truncated.";
                    break;
                }

                _printDocument!.AddPage(CreatePrintPage(bitmap, pageIndex, e.PrintTaskOptions));
            }
        }
        catch (Exception)
        {
            // async void: an unhandled exception here crashes the process.
        }
        finally
        {
            // Always complete, even truncated — a job with pending pages
            // leaves the print dialog waiting forever.
            _printDocument!.AddPagesComplete();
        }
    }

    private Task<WriteableBitmap?> GetPreviewBitmapAsync(PrintJob job, int pageIndex)
    {
        // Cache the task, not the result, so a page requested twice while the
        // first render is in flight shares it. Only touched on the UI thread.
        if (!job.PreviewPages.TryGetValue(pageIndex, out var pending))
        {
            pending = RenderPrintBitmapAsync(job, pageIndex, RenderDpi);
            job.PreviewPages[pageIndex] = pending;
        }

        return pending;
    }

    private async Task<WriteableBitmap?> RenderPrintBitmapAsync(PrintJob job, int pageIndex, uint dpi)
    {
        var rendered = await _facade.RenderPageForPrintAsync(job.SessionId, (uint)pageIndex, dpi, false);
        if (!rendered.IsSuccess)
        {
            return null;
        }

        return await MaterializeBitmapAsync(rendered.Value!);
    }

    private Grid CreatePrintPage(WriteableBitmap bitmap, int pageIndex, PrintTaskOptions options)
    {
        var description = options.GetPageDescription((uint)pageIndex);
        var page = new Grid
        {
            Width = description.PageSize.Width,
            Height = description.PageSize.Height,
        };
        var imageable = description.ImageableRect;
        page.Children.Add(new Image
        {
            Source = bitmap,
            Width = imageable.Width,
            Height = imageable.Height,
            Margin = new Thickness(imageable.X, imageable.Y, 0, 0),
            HorizontalAlignment = HorizontalAlignment.Left,
            VerticalAlignment = VerticalAlignment.Top,
            Stretch = Stretch.Uniform,
        });
        return page;
    }

    private async void SearchResultsList_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_session is null || SearchResultsList.SelectedItem is not ListViewItem { Tag: SearchHit hit })
        {
            return;
        }

        var navigation = await _facade.NavigateToSearchResultAsync(_session.SessionId, hit);
        if (!navigation.IsSuccess)
        {
            ShowError(navigation.Error!);
            return;
        }

        _session = navigation.Value!;
        var pageIndex = checked((int)hit.PageIndex);
        PageScroller.ChangeView(null, _slots[pageIndex].Top, null, disableAnimation: false);
        ShowSearchHighlight(pageIndex, hit.CharacterBounds);
    }

    private void ClearSearchResults()
    {
        SearchResultsList.Items.Clear();
        SearchStatus.Text = "";
        foreach (var slot in _slots)
        {
            slot.Highlights.Children.Clear();
        }
    }

    private void ShowSearchHighlight(int pageIndex, IReadOnlyList<SearchRect> bounds)
    {
        foreach (var slot in _slots)
        {
            slot.Highlights.Children.Clear();
        }

        var page = _session!.Pages[pageIndex];
        var overlay = _slots[pageIndex].Highlights;
        foreach (var boundsRect in bounds)
        {
            var rectangle = new Rectangle
            {
                Width = Math.Max(1, boundsRect.WidthPt * PointsToDips),
                Height = Math.Max(1, boundsRect.HeightPt * PointsToDips),
                Fill = new SolidColorBrush(global::Windows.UI.Color.FromArgb(96, 255, 214, 10)),
                Stroke = new SolidColorBrush(Microsoft.UI.Colors.DarkOrange),
                StrokeThickness = 1,
            };
            Canvas.SetLeft(rectangle, boundsRect.XPt * PointsToDips);
            Canvas.SetTop(rectangle, (page.HeightPt - boundsRect.YPt - boundsRect.HeightPt) * PointsToDips);
            overlay.Children.Add(rectangle);
        }
    }

    /// <summary>
    /// Copies the renderer's RGBA pixels straight into a WriteableBitmap
    /// (swizzled to BGRA off the UI thread) — no encode/decode round trip.
    /// </summary>
    private static async Task<WriteableBitmap> MaterializeBitmapAsync(RenderedPage page)
    {
        var bgra = await Task.Run(() =>
        {
            var rgba = page.Rgba;
            var converted = new byte[rgba.Length];
            for (var i = 0; i < rgba.Length; i += 4)
            {
                converted[i] = rgba[i + 2];
                converted[i + 1] = rgba[i + 1];
                converted[i + 2] = rgba[i];
                converted[i + 3] = rgba[i + 3];
            }

            return converted;
        });

        var bitmap = new WriteableBitmap((int)page.Width, (int)page.Height);
        using (var stream = bitmap.PixelBuffer.AsStream())
        {
            await stream.WriteAsync(bgra);
        }

        bitmap.Invalidate();
        return bitmap;
    }

    private void ShowEmpty(string message)
    {
        EmptyState.Text = message;
        EmptyState.Visibility = Visibility.Visible;
        ErrorState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
        PageCounter.Text = "";
    }

    private void ShowError(UserSafeError error)
    {
        ErrorState.Text = $"{error.Message} Reference: {error.CorrelationId}";
        ErrorState.Visibility = Visibility.Visible;
        EmptyState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
    }

    private void SetBusy(bool isBusy)
    {
        BusyIndicator.IsActive = isBusy;
        BusyIndicator.Visibility = isBusy ? Visibility.Visible : Visibility.Collapsed;
        OpenButton.IsEnabled = !isBusy;
        PrintButton.IsEnabled = !isBusy;
        SearchButton.IsEnabled = !isBusy;
    }

    private sealed class PageSlot(Border container, Image image, Canvas highlights, double top, double height)
    {
        public Border Container { get; } = container;
        public Image Image { get; } = image;
        public Canvas Highlights { get; } = highlights;
        public double Top { get; } = top;
        public double Height { get; } = height;
        public bool Requested { get; set; }
        public bool Rendered { get; set; }
    }

    private sealed class PrintJob(string sessionId, string displayName, int pageCount)
    {
        public string SessionId { get; } = sessionId;
        public string DisplayName { get; } = displayName;
        public int PageCount { get; } = pageCount;
        /// <summary>Per-page preview renders, keyed by page index. UI thread only.</summary>
        public Dictionary<int, Task<WriteableBitmap?>> PreviewPages { get; } = [];
    }
}
