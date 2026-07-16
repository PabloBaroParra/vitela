using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow : Window
{
    private const uint RenderDpi = 144;
    private const double PointsToDips = RenderDpi / 72.0;
    private const double PageSpacing = 12;
    /// <summary>Rendered pages kept alive beyond the visible range, per side.</summary>
    private const int KeepWindow = 2;

    private readonly PdfDocumentFacade _facade = new(new GeneratedPdfCore(), new DebugDiagnosticLogger());
    private DocumentSession? _session;
    private List<PageSlot> _slots = [];

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
        OperationResult<DocumentSession> result;
        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out var bytes);
            result = await _facade.OpenAsync(new DocumentSource(file.Name, bytes));
        }
        catch (Exception error)
        {
            result = _facade.OpenReadFailure(error);
        }

        SetBusy(false);
        if (!result.IsSuccess)
        {
            ShowError(result.Error!);
            return;
        }

        _session = result.Value!;
        DocumentTitle.Text = _session.DisplayName;
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
            var container = new Border
            {
                Width = page.WidthPt * PointsToDips,
                Height = page.HeightPt * PointsToDips,
                BorderThickness = new Thickness(1),
                BorderBrush = new SolidColorBrush(Microsoft.UI.Colors.Gray),
                // Unrendered pages read as paper, not as a hole in the theme.
                Background = new SolidColorBrush(Microsoft.UI.Colors.White),
                Child = image,
            };
            _slots.Add(new PageSlot(container, image, top, container.Height));
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
    }

    private sealed class PageSlot(Border container, Image image, double top, double height)
    {
        public Border Container { get; } = container;
        public Image Image { get; } = image;
        public double Top { get; } = top;
        public double Height { get; } = height;
        public bool Requested { get; set; }
        public bool Rendered { get; set; }
    }
}
