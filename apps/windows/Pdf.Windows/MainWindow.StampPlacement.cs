using Pdf.Windows.Facade;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>Chooses image bytes for a stamp; the core owns insertion and history.</summary>
public sealed partial class MainWindow
{
    private async Task<StorageFile?> PickStampImageAsync()
    {
        var picker = new FileOpenPicker();
        foreach (var extension in new[] { ".png", ".jpg", ".jpeg" }) picker.FileTypeFilter.Add(extension);
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        return await picker.PickSingleFileAsync();
    }

    private async Task PlaceImageStampAsync(uint pageIndex, PdfCoreRect rect, Func<Task<StorageFile?>>? pickImage = null)
    {
        if (_session is not { } session || _isBusy || _organizing || _dialogOpen
            || _annotationState?.EditingAllowed != true) return;

        var snapshot = _annotationState;
        SetBusy(true);
        try
        {
            StorageFile? file;
            byte[] bytes;
            try
            {
                file = await (pickImage ?? PickStampImageAsync)();
                if (_session?.SessionId != session.SessionId) return;
                if (file is null)
                {
                    AnnotationStatus.Text = "Stamp placement canceled.";
                    return;
                }
                var buffer = await FileIO.ReadBufferAsync(file);
                CryptographicBuffer.CopyToByteArray(buffer, out bytes);
            }
            catch (Exception)
            {
                ReportDropFailure(session.SessionId, "The selected stamp image could not be read.");
                return;
            }

            // Neither an old page rectangle nor old image bytes may reach a replacement document.
            if (_session?.SessionId != session.SessionId || !ReferenceEquals(_annotationState, snapshot) || _organizing
                || _annotationState?.EditingAllowed != true) return;
            await InsertStampFromImageBytesAsync(session.SessionId, pageIndex, rect, bytes);
        }
        finally
        {
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }
}
