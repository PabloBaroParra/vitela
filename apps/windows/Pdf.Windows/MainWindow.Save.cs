using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Localization;
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
                CommitButtonText = I18n.T("write-accept"),
            };
            picker.FileTypeChoices.Add("PDF", [".pdf"]);
            InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
            await SuggestOriginAsync(picker);
            var file = await picker.PickSaveFileAsync();
            if (file is null)
            {
                AnnotationStatus.Text = I18n.T("save-cancelled");
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
            AnnotationStatus.Text = I18n.T("save-done-editable");
            _documentOrigin = file.Path;
            ForgetRecoveryEntry();
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

    /// <summary>
    /// Pre-selects the file the document came from — for a restored document,
    /// the one that crashed — so Save lands where the reader expects. It stays a
    /// picker with the usual overwrite confirmation; a document with no origin
    /// keeps the plain suggestion.
    /// </summary>
    private async Task SuggestOriginAsync(FileSavePicker picker)
    {
        // A storage item without a file-system path reports an empty one.
        if (string.IsNullOrWhiteSpace(_documentOrigin) || !Path.IsPathFullyQualified(_documentOrigin)) return;
        var origin = _documentOrigin;
        var name = Path.GetFileNameWithoutExtension(origin);
        if (name.Length == 0) return;
        picker.SuggestedFileName = name;
        try
        {
            picker.SuggestedSaveFile = await StorageFile.GetFileFromPathAsync(origin);
        }
        catch (Exception)
        {
            // Moved, deleted or unreachable: the name alone is still the best suggestion.
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
            Title = sharing ? I18n.T("write-signature-share-title") : I18n.T("write-signature-title"),
            Content = new TextBlock
            {
                Text = sharing ? I18n.T("write-signature-share-detail") : I18n.T("write-signature-detail"),
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = sharing ? I18n.T("write-signature-share-proceed") : I18n.T("write-signature-proceed"),
            CloseButtonText = I18n.T("write-signature-cancel"),
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };
        if (await ShowModalAsync(dialog) == ContentDialogResult.Primary) return true;
        AnnotationStatus.Text = compressing ? I18n.T("compress-cancelled")
            : protecting ? I18n.T("protect-cancelled")
            : sharing ? I18n.T("share-cancelled")
            : I18n.T("save-cancelled");
        return null;
    }
}
