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
/// Files and images arriving from outside the app, and the meanings a drop can
/// have: a PDF opens as the document, an image lands on a page as a stamp.
/// Ctrl+V takes the clipboard down the same stamp path.
///
/// There are two drop targets rather than one. The page canvases claim their
/// own drops because a stamp has to know which page and which point it was
/// aimed at; the window claims everything else, which is what lets a PDF be
/// dropped when nothing is open yet and there is no page to aim at. A page
/// drop marks itself handled so a single gesture never reaches both.
/// </summary>
public sealed partial class MainWindow
{
    private const double DefaultStampWidthPt = 144.0;
    private const double DefaultStampHeightPt = 36.0;
    private const string UnsupportedDropMessage = "Vitela opens PDF files. Drop a PNG or JPEG onto a page to add it as a stamp.";

    /// <summary>Default placement anchored at the drop point's top-left corner, matching the Linux shell's <c>stamp_rect</c>.</summary>
    private static PdfCoreRect DefaultStampRect(AnnotationPoint point) =>
        new(point.X, point.Y - DefaultStampHeightPt, DefaultStampWidthPt, DefaultStampHeightPt);

    private void ConnectFileDrop(PageSlot slot, int pageIndex)
    {
        slot.Highlights.AllowDrop = true;
        slot.Highlights.DragOver += (_, args) => AcceptFileDrop(args);
        slot.Highlights.Drop += async (_, args) => await DropOnPageAsync(slot, pageIndex, args);
    }

    private static void AcceptFileDrop(DragEventArgs args)
    {
        if (!args.DataView.Contains(StandardDataFormats.StorageItems)) return;
        args.AcceptedOperation = DataPackageOperation.Copy;
        args.Handled = true;
    }

    private void Window_DragOver(object sender, DragEventArgs args) => AcceptFileDrop(args);

    /// <summary>
    /// A drop that missed every page. Only a PDF has an unambiguous meaning
    /// here — and it is the case that has to work with nothing open at all,
    /// where no page exists to receive a stamp.
    /// </summary>
    private async void Window_Drop(object sender, DragEventArgs args)
    {
        args.Handled = true;
        if (_isBusy) return;
        var deferral = args.GetDeferral();
        try
        {
            if (await FirstActionableFileAsync(args.DataView) is not { } dropped)
            {
                AnnotationStatus.Text = UnsupportedDropMessage;
            }
            else if (dropped.Kind == DroppedFileKind.Document)
            {
                await OpenStorageFileAsync(dropped.Item);
            }
            else
            {
                AnnotationStatus.Text = _session is null
                    ? "Open a PDF first, then drop an image onto a page to stamp it."
                    : "Drop the image onto a page to place it as a stamp.";
            }
        }
        finally
        {
            deferral.Complete();
        }
    }

    /// <summary>
    /// A drop that landed on a page. Reading the file outlives the handler, so
    /// the drop is held open with a deferral — without one the source is free
    /// to tear the <see cref="DragEventArgs.DataView"/> down the moment this
    /// returns. The drop point is read before the first await, while the event
    /// still knows where the pointer was.
    /// </summary>
    private async Task DropOnPageAsync(PageSlot slot, int pageIndex, DragEventArgs args)
    {
        args.Handled = true;
        if (_isBusy) return;
        var deferral = args.GetDeferral();
        try
        {
            var point = ToPdf(slot, pageIndex, args.GetPosition(slot.Highlights));
            if (await FirstActionableFileAsync(args.DataView) is not { } dropped)
            {
                AnnotationStatus.Text = UnsupportedDropMessage;
            }
            else if (dropped.Kind == DroppedFileKind.Document)
            {
                // Dropping a PDF onto an open page means "open this instead",
                // not "stamp it" — so it takes the same route as the picker,
                // unsaved-changes guard included.
                await OpenStorageFileAsync(dropped.Item);
            }
            else
            {
                await StampDroppedImageAsync(dropped.Item, pageIndex, point);
            }
        }
        finally
        {
            deferral.Complete();
        }
    }

    /// <summary>
    /// Reads the dropped image and hands it to the shared stamp path. The
    /// permission check lives here rather than at the drop, because a read-only
    /// document still accepts a dropped PDF — it just cannot be annotated.
    /// </summary>
    private async Task StampDroppedImageAsync(StorageFile file, int pageIndex, AnnotationPoint point)
    {
        if (_session is not { } session) return;
        if (_annotationState?.EditingAllowed != true)
        {
            AnnotationStatus.Text = "This document does not allow annotation edits.";
            return;
        }

        try
        {
            var buffer = await FileIO.ReadBufferAsync(file);
            CryptographicBuffer.CopyToByteArray(buffer, out byte[] imageBytes);
            await InsertStampFromImageBytesAsync(session.SessionId, (uint)pageIndex, DefaultStampRect(point), imageBytes);
        }
        catch (Exception)
        {
            ReportDropFailure(session.SessionId, "The dropped image could not be read.");
        }
    }

    /// <summary>
    /// The one file out of the drop the shell will act on, or null when the
    /// payload holds no files at all — dragged text, a link, a folder.
    /// </summary>
    private static async Task<(StorageFile Item, DroppedFileKind Kind)?> FirstActionableFileAsync(DataPackageView data)
    {
        if (!data.Contains(StandardDataFormats.StorageItems)) return null;
        try
        {
            var items = await data.GetStorageItemsAsync();
            return FileDropRouting.FirstActionable(items.OfType<StorageFile>(), file => file.Path);
        }
        catch (Exception)
        {
            return null;
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
