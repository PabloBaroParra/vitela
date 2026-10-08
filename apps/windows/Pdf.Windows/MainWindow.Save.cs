using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

public sealed partial class MainWindow
{
    private bool _savingDocument;
    private async void SaveButton_Click(object sender, RoutedEventArgs e) => await SaveToPickedFileAsync();

    /// <summary>A continuation may discard work only after the destination write succeeds.</summary>
    private async Task<bool> SaveToPickedFileAsync()
    {
        if (_savingDocument || _isBusy || _dialogOpen || _session is not { } session) return false;
        _savingDocument = true;
        try
        {
            SetBusy(true);
            if (!await PrepareDocumentLifecycleAsync()) return false;
            var picker = new FileSavePicker
            {
                SuggestedFileName = "document",
                DefaultFileExtension = ".pdf",
                CommitButtonText = "Save",
            };
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = "Save cancelled.";
                return false;
            }
            if (_session?.SessionId != session.SessionId) return false;
            var acknowledged = await AskSignatureLossAsync();
            if (acknowledged is null)
            {
                return false;
            }
            if (_session?.SessionId != session.SessionId) return false;
            SetBusy(true);
            return await WritePickedPdfAsync(session.SessionId, file, acknowledged.Value);
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
            return false;
        }
        finally
        {
            _savingDocument = false;
            SetBusy(false);
            RestoreAnnotationControls();
        }
    }

    private async Task<bool> WritePickedPdfAsync(string sessionId, StorageFile file, bool acknowledged)
    {
        StorageFile? temporary = null;
        try
        {
            var result = await _facade.SaveToDestinationAsync(sessionId, async bytes =>
            {
                var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(file.Path)!);
                temporary = await folder.CreateFileAsync($".{file.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
                await FileIO.WriteBytesAsync(temporary, bytes);
                await temporary.MoveAndReplaceAsync(file);
                temporary = null;
            }, acknowledged);
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return false;
            }
            AnnotationStatus.Text = "PDF saved. Changes remain editable in this session.";
            return true;
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
            return false;
        }
        finally
        {
            try { if (temporary is not null) await temporary.DeleteAsync(); }
            catch (Exception error) { AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message; }
        }
    }

    /// <summary>Cancel is the default; scan failure never implies an unsigned source.</summary>
    private async Task<bool?> AskSignatureLossAsync(bool protecting = false, bool compressing = false, bool sharing = false)
    {
        if (_session is not { } session) return null;
        SetBusy(true);
        var query = compressing
            ? await _facade.CompressionWillInvalidateSignaturesAsync(session.SessionId)
            : protecting
            ? await _facade.ProtectionWillInvalidateSignaturesAsync(session.SessionId)
            : await _facade.WillInvalidateSignaturesAsync(session.SessionId);
        SetBusy(false);
        if (!query.IsSuccess)
        {
            AnnotationStatus.Text = query.Error!.Message;
            return null;
        }
        if (!query.Value) return false;
        var dialog = new ContentDialog
        {
            Title = sharing ? "Sharing will break this document's signature" : "Saving will break this document's signature",
            Content = new TextBlock
            {
                Text = sharing
                    ? "This document is signed. Sharing sends a rewritten copy, so its signature will no longer match what it covers."
                        + "\n\nIt is not removed: the copy still carries the signature, and PDF readers will report it as invalid rather than missing."
                        + "\n\nTo send a copy that still verifies, cancel and share the original file instead."
                    : "This document is signed. Saving rewrites the file, so the signature will no longer match what it covers."
                        + "\n\nIt is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather than missing."
                        + "\n\nTo keep a copy that still verifies, cancel and save to a different file.",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = sharing ? "Share anyway" : "Save anyway",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return true;
        AnnotationStatus.Text = compressing ? "Compression cancelled." : protecting ? "Protection cancelled." : sharing ? "Share cancelled." : "Save cancelled.";
        return null;
    }
}
