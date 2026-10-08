using Microsoft.UI.Xaml;
using Pdf.Windows.Viewer;
using System.Runtime.InteropServices;
using Windows.ApplicationModel.DataTransfer;
using Windows.Storage;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Share through the Windows share sheet (Nearby Sharing, Phone Link, mail and
/// any installed share target). The copy is written as a save would write it,
/// pending edits included, to a private temp folder the target reads from.
/// </summary>
/// <remarks>
/// A desktop window has no CoreWindow, so the manager is reached through
/// <see cref="DataTransferManagerInterop"/> by window handle — the same route
/// printing takes through PrintManagerInterop.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>
    /// How long a shared copy outlives its share. The sheet returns as soon as
    /// it is shown, and a target may still be reading the file well after; a
    /// later share only sweeps copies older than this.
    /// </summary>
    private static readonly TimeSpan ShareCopyLifetime = TimeSpan.FromHours(1);

    private static string ShareRoot => Path.Combine(Path.GetTempPath(), "Vitela", "Share");

    private DataTransferManager? _shareManager;
    private (StorageFile File, string Title)? _sharedCopy;
    private bool _sharingDocument;

    private async void ShareButton_Click(object sender, RoutedEventArgs e)
    {
        if (_sharingDocument || _isBusy || _dialogOpen || _shellPickingFile) return;
        if (_session is not { PageCount: > 0 } session)
        {
            AnnotationStatus.Text = "Open a PDF before sharing.";
            return;
        }
        _sharingDocument = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync() || _session?.SessionId != session.SessionId) return;
            var acknowledged = await AskSignatureLossAsync(sharing: true);
            if (acknowledged is null || _session?.SessionId != session.SessionId) return;
            SetBusy(true);
            var shared = await _facade.ShareBytesAsync(session.SessionId, acknowledged.Value);
            if (!shared.IsSuccess)
            {
                AnnotationStatus.Text = shared.Error!.Message;
                return;
            }
            var fileName = ShareFileName.For(session.DisplayName);
            _sharedCopy = (await WriteShareCopyAsync(fileName, shared.Value!), fileName);
            var window = WindowNative.GetWindowHandle(this);
            EnsureShareManager(window);
            DataTransferManagerInterop.ShowShareUIForWindow(window);
            AnnotationStatus.Text = string.Empty;
        }
        catch (Exception error) when (error is COMException or NotImplementedException)
        {
            AnnotationStatus.Text = "Sharing is unavailable on this Windows installation.";
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = $"Could not share: {error.Message}";
        }
        finally
        {
            _sharingDocument = false;
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private void EnsureShareManager(IntPtr window)
    {
        if (_shareManager is not null) return;
        _shareManager = DataTransferManagerInterop.GetForWindow(window);
        _shareManager.DataRequested += ShareManager_DataRequested;
    }

    /// <summary>The sheet asks for the payload after it opens, on its own schedule.</summary>
    private void ShareManager_DataRequested(DataTransferManager sender, DataRequestedEventArgs args)
    {
        if (_sharedCopy is not { } copy)
        {
            args.Request.FailWithDisplayText("There is no document to share.");
            return;
        }
        args.Request.Data.Properties.Title = copy.Title;
        args.Request.Data.SetStorageItems([copy.File], readOnly: true);
    }

    /// <summary>One fresh folder per share, so two shares never race over one file name.</summary>
    private static async Task<StorageFile> WriteShareCopyAsync(string fileName, byte[] bytes)
    {
        SweepOldShareCopies();
        var folderPath = Path.Combine(ShareRoot, Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(folderPath);
        var filePath = Path.Combine(folderPath, fileName);
        await File.WriteAllBytesAsync(filePath, bytes);
        return await StorageFile.GetFileFromPathAsync(filePath);
    }

    private static void SweepOldShareCopies()
    {
        if (!Directory.Exists(ShareRoot)) return;
        foreach (var folder in Directory.EnumerateDirectories(ShareRoot))
        {
            try
            {
                if (DateTime.UtcNow - Directory.GetCreationTimeUtc(folder) > ShareCopyLifetime) Directory.Delete(folder, recursive: true);
            }
            // A target still holding the file keeps it for the next sweep.
            catch (Exception error) when (error is IOException or UnauthorizedAccessException) { }
        }
    }
}
