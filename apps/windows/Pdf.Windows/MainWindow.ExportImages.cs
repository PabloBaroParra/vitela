using Microsoft.UI.Xaml;
using Windows.Graphics.Imaging;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>Exports every page to its own PNG without replacing the open session.</summary>
public sealed partial class MainWindow
{
    private const uint ExportDpi = 150;

    private async void ExportImagesButton_Click(object sender, RoutedEventArgs e)
    {
        if (_isBusy || _session is not { PageCount: > 0 } session) return;

        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        StorageFolder? folder;
        try { folder = await picker.PickSingleFolderAsync(); }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
            return;
        }
        if (folder is null) return;

        SetBusy(true);
        var written = 0u;
        var stem = Path.GetFileNameWithoutExtension(session.DisplayName);
        var digits = session.PageCount.ToString().Length;
        StorageFile? incomplete = null;
        try
        {
            for (uint index = 0; index < session.PageCount; index++)
            {
                AnnotationStatus.Text = $"Exporting page {index + 1} of {session.PageCount}...";
                var rendered = await _facade.RenderPageForPrintAsync(session.SessionId, index, ExportDpi, false);
                if (!rendered.IsSuccess)
                {
                    AnnotationStatus.Text = $"Page {index + 1} could not be exported: {rendered.Error?.Message ?? "No image was rendered."} {written} pages written.";
                    return;
                }

                var page = rendered.Value!;
                var name = $"{stem}-{(index + 1).ToString().PadLeft(digits, '0')}.png";
                incomplete = await folder.CreateFileAsync(name, CreationCollisionOption.GenerateUniqueName);
                using (var stream = await incomplete.OpenAsync(FileAccessMode.ReadWrite))
                {
                    var encoder = await BitmapEncoder.CreateAsync(BitmapEncoder.PngEncoderId, stream);
                    encoder.SetPixelData(BitmapPixelFormat.Rgba8, BitmapAlphaMode.Straight,
                        page.Width, page.Height, ExportDpi, ExportDpi, page.Rgba);
                    await encoder.FlushAsync();
                }
                incomplete = null;
                written++;
            }

            AnnotationStatus.Text = $"Exported {written} pages as PNG to {folder.Path}.";
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = $"Export stopped after {written} pages: {_facade.SaveWriteFailure(error).Error!.Message}";
        }
        finally
        {
            if (incomplete is not null)
            {
                try { await incomplete.DeleteAsync(); }
                catch { }
            }
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
