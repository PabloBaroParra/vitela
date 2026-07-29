using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Printing;
using Microsoft.UI.Xaml;
using System.Runtime.InteropServices;
using Windows.Graphics.Printing;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Native printing through <see cref="PrintManager"/>: preview pages render
/// lazily at screen DPI, and the full-quality pass at <see cref="PrintDpi"/>
/// only runs once the user confirms the job.
/// </summary>
public sealed partial class MainWindow
{
    private const uint PrintDpi = 300;
    /// <summary>
    /// Preview pages render at a fixed screen-grade DPI: the print preview is
    /// scaled to the dialog's own page description, so it is independent of
    /// whatever zoom the viewer is currently showing.
    /// </summary>
    private const uint PreviewDpi = 144;

    private PrintManager? _printManager;
    private PrintDocument? _printDocument;
    // Captured on the UI thread: PrintTaskRequested fires on a print worker
    // thread, and reading PrintDocument.DocumentSource there throws
    // RPC_E_WRONG_THREAD, leaving the preview stuck on "loading".
    private IPrintDocumentSource? _printDocumentSource;
    private PrintJob? _printJob;
    private PrintTaskOptions? _printTaskOptions;

    private async void PrintButton_Click(object sender, RoutedEventArgs e)
    {
        if (_session is null || _session.State == Facade.DocumentSessionState.Empty)
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
            pending = RenderPrintBitmapAsync(job, pageIndex, PreviewDpi);
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

    private sealed class PrintJob(string sessionId, string displayName, int pageCount)
    {
        public string SessionId { get; } = sessionId;
        public string DisplayName { get; } = displayName;
        public int PageCount { get; } = pageCount;
        /// <summary>Per-page preview renders, keyed by page index. UI thread only.</summary>
        public Dictionary<int, Task<WriteableBitmap?>> PreviewPages { get; } = [];
    }
}
