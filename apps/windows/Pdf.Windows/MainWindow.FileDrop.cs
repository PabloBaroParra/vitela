using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.ApplicationModel.DataTransfer;
using Windows.Graphics.Imaging;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Streams;

namespace Pdf.Windows;

/// <summary>
/// Images arriving from outside the app — dragged onto a page, or pasted from
/// the clipboard. Both land on the shared insertion path in
/// <c>MainWindow.Annotations.cs</c>, so a stamp behaves the same however it got
/// here, preview and undo entry included.
/// </summary>
public sealed partial class MainWindow
{
    private const double DefaultStampWidthPt = 144.0;
    private const double DefaultStampHeightPt = 36.0;

    /// <summary>Default placement anchored at the drop point's top-left corner, matching the Linux shell's <c>stamp_rect</c>.</summary>
    private static PdfCoreRect DefaultStampRect(AnnotationPoint point) =>
        new(point.X, point.Y - DefaultStampHeightPt, DefaultStampWidthPt, DefaultStampHeightPt);

    private void ConnectFileDrop(PageSlot slot, int pageIndex)
    {
        slot.Highlights.AllowDrop = true;
        slot.Highlights.DragOver += (_, args) => AcceptFileDrop(args);
        slot.Highlights.Drop += async (_, args) => await DropImageStampAsync(slot, pageIndex, args);
    }

    private static void AcceptFileDrop(DragEventArgs args)
    {
        if (!args.DataView.Contains(StandardDataFormats.StorageItems)) return;
        args.AcceptedOperation = DataPackageOperation.Copy;
        args.Handled = true;
    }

    /// <summary>
    /// Reading the dropped file outlives the handler, so the drop is held open
    /// with a deferral — without one the source is free to tear the
    /// <see cref="DragEventArgs.DataView"/> down the moment this returns. The
    /// drop point is read before the first await, while the event still knows
    /// where the pointer was.
    /// </summary>
    private async Task DropImageStampAsync(PageSlot slot, int pageIndex, DragEventArgs args)
    {
        args.Handled = true;
        if (_session is not { } session || _annotationState?.EditingAllowed != true) return;
        var deferral = args.GetDeferral();
        try
        {
            var point = ToPdf(slot, pageIndex, args.GetPosition(slot.Highlights));
            var items = await args.DataView.GetStorageItemsAsync();
            var file = items.OfType<StorageFile>().FirstOrDefault();
            if (file is null || string.IsNullOrWhiteSpace(file.Path) || !ImageStampInput.HasSupportedFileExtension(file.Path))
            {
                ReportDropFailure(session.SessionId, "Only local PNG and JPEG files can be dropped onto a page.");
                return;
            }

            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out byte[] imageBytes);
            await InsertStampFromImageBytesAsync(session.SessionId, (uint)pageIndex, DefaultStampRect(point), imageBytes);
        }
        catch (Exception)
        {
            ReportDropFailure(session.SessionId, "The dropped image could not be read.");
        }
        finally
        {
            deferral.Complete();
        }
    }

    /// <summary>Reports a drop failure only if the document it was aimed at is still open.</summary>
    private void ReportDropFailure(string sessionId, string message)
    {
        if (ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) AnnotationStatus.Text = message;
    }

    /// <summary>
    /// Ctrl+V stamps a clipboard bitmap onto the page in view. Left alone the
    /// accelerator would swallow the paste every text field in the window
    /// expects, starting with <see cref="SearchBox"/>, so it stands down
    /// whenever an editable control holds focus.
    /// </summary>
    private async void PasteImage_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (TextInputHasFocus())
        {
            args.Handled = false;
            return;
        }

        args.Handled = true;
        if (_session is not { State: DocumentSessionState.Ready } session || _annotationState?.EditingAllowed != true || session.Pages.Count == 0)
        {
            AnnotationStatus.Text = "Open an editable PDF before pasting an image.";
            return;
        }
        var content = Clipboard.GetContent();
        if (!content.Contains(StandardDataFormats.Bitmap))
        {
            AnnotationStatus.Text = "Clipboard does not contain a bitmap image.";
            return;
        }

        try
        {
            var imageBytes = await ReadClipboardBitmapAsPngAsync(content);
            var pageIndex = (uint)Math.Clamp(_firstVisiblePage, 0, session.Pages.Count - 1);
            var page = session.Pages[(int)pageIndex];
            await InsertStampFromImageBytesAsync(session.SessionId, pageIndex, DefaultStampRect(new AnnotationPoint(page.WidthPt / 2, page.HeightPt / 2)), imageBytes);
        }
        catch (Exception)
        {
            ReportDropFailure(session.SessionId, "The clipboard bitmap could not be used.");
        }
    }

    /// <summary>
    /// Re-encodes the clipboard bitmap as PNG. The clipboard hands over a
    /// decoded surface rather than an encoded file, and the core's stamp
    /// builder takes encoded bytes.
    /// </summary>
    private static async Task<byte[]> ReadClipboardBitmapAsPngAsync(DataPackageView content)
    {
        var bitmap = await content.GetBitmapAsync();
        using var input = await bitmap.OpenReadAsync();
        var decoder = await BitmapDecoder.CreateAsync(input);
        var pixels = await decoder.GetPixelDataAsync(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied, new BitmapTransform(), ExifOrientationMode.RespectExifOrientation, ColorManagementMode.DoNotColorManage);
        using var output = new InMemoryRandomAccessStream();
        var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, output);
        encoder.SetPixelData(BitmapPixelFormat.Bgra8, BitmapAlphaMode.Premultiplied, decoder.PixelWidth, decoder.PixelHeight, decoder.DpiX, decoder.DpiY, pixels.DetachPixelData());
        await encoder.FlushAsync();
        output.Seek(0);
        var buffer = await output.ReadAsync(new global::Windows.Storage.Streams.Buffer((uint)output.Size), (uint)output.Size, InputStreamOptions.None);
        CryptographicBuffer.CopyToByteArray(buffer, out byte[] imageBytes);
        return imageBytes;
    }

    /// <summary>
    /// Whether an editable control owns the keystroke. Window-wide accelerators
    /// are resolved ahead of the focus chain, and
    /// <see cref="KeyboardAcceleratorInvokedEventArgs.Handled"/> defaults to
    /// <c>true</c>, so every accelerator that collides with ordinary typing has
    /// to stand down here or it takes the key mid-word.
    /// </summary>
    private bool TextInputHasFocus() =>
        _xamlRoot is { } root && FocusManager.GetFocusedElement(root) is TextBox or RichEditBox or AutoSuggestBox or PasswordBox;
}
