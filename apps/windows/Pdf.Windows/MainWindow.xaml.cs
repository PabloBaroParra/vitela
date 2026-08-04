using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Pdf.Windows.Facade;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Security.Cryptography;
using Windows.Storage;
using Windows.Storage.Pickers;
using WinRT.Interop;

namespace Pdf.Windows;

/// <summary>
/// Shell bootstrap: the state every feature shares, the document open path,
/// and the empty/error/busy chrome. Feature logic lives in one partial per
/// responsibility — <c>MainWindow.Viewer.cs</c> (page layout, zoom and
/// rendering), <c>MainWindow.Search.cs</c>, <c>MainWindow.Print.cs</c>.
/// </summary>
public sealed partial class MainWindow : Window
{
    /// <summary>
    /// The sample document, copied next to the executable by the csproj from
    /// the shared <c>assets/sample/</c> directory. Read from the base
    /// directory rather than an <c>ms-appx:///</c> URI because this shell is
    /// unpackaged (<c>WindowsPackageType=None</c>), where that URI scheme is
    /// unavailable.
    /// </summary>
    private static readonly string SamplePath = Path.Combine(AppContext.BaseDirectory, "Assets", "vitela-sample.pdf");
    private const string SampleDisplayName = "Vitela sample.pdf";

    /// <summary>
    /// Encrypted samples for exercising the password prompt, sourced from the
    /// same <c>tests/fixtures/encrypted/</c> corpus <c>pdf-manip</c>'s decrypt
    /// tests use (see <c>tests/fixtures/README.md</c>). User passwords:
    /// <c>user-aes-pass</c> / <c>user-rc4-pass</c>.
    /// </summary>
    private static readonly string Aes128SamplePath = Path.Combine(AppContext.BaseDirectory, "Assets", "aes_128_user_and_owner.pdf");
    private const string Aes128SampleDisplayName = "AES-128 sample.pdf";
    private static readonly string Rc4128SamplePath = Path.Combine(AppContext.BaseDirectory, "Assets", "rc4_128_user_and_owner.pdf");
    private const string Rc4128SampleDisplayName = "RC4-128 sample.pdf";

    private readonly PdfDocumentFacade _facade = new(new GeneratedPdfCore(), new FileDiagnosticLogger(FileDiagnosticLogger.DefaultPath));
    private DocumentSession? _session;
    private bool _isBusy;

    public MainWindow()
    {
        InitializeComponent();
        Activated += MainWindow_Activated;
    }

    private void MainWindow_Activated(object sender, WindowActivatedEventArgs e)
    {
        if (_xamlRoot is not null || Content.XamlRoot is not { } xamlRoot)
        {
            return;
        }

        _xamlRoot = xamlRoot;
        _rasterizationScale = xamlRoot.RasterizationScale;
        xamlRoot.Changed += XamlRoot_Changed;
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

        await OpenStorageFileAsync(file);
    }

    /// <summary>
    /// Reads a PDF the user chose and hands it to the shared open path. Both
    /// ways in — the picker and a drop on the window — go through here, so
    /// neither can drift from the other on read failures, the unsaved-changes
    /// guard, or the password retry.
    /// </summary>
    private async Task OpenStorageFileAsync(StorageFile file)
    {
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
            ReportFailedOpen(_facade.OpenReadFailure(error).Error!);
            return;
        }

        await OpenDocumentAsync(file.Name, bytes);
    }

    private async void SaveButton_Click(object sender, RoutedEventArgs e) => await SaveToPickedFileAsync();

    /// <summary>
    /// Saves through the file picker and reports whether the document reached
    /// disk. The answer matters to <see cref="OpenDocumentAsync"/>: when the
    /// reader chose Save to get past the pending-edit prompt, a cancelled
    /// picker or a failed write has to abandon the open too, or the work they
    /// just asked to keep would be dropped anyway.
    /// </summary>
    private async Task<bool> SaveToPickedFileAsync()
    {
        if (_session is null) return false;
        var picker = new FileSavePicker();
        picker.FileTypeChoices.Add("PDF", [".pdf"]);
        picker.SuggestedFileName = Path.GetFileNameWithoutExtension(_session.DisplayName);
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSaveFileAsync();
        if (file is null) return false;
        SetBusy(true);
        StorageFile? temporary = null;
        var saved = false;
        try
        {
            var result = await _facade.SaveToDestinationAsync(_session.SessionId, async bytes =>
            {
                var folder = await StorageFolder.GetFolderFromPathAsync(Path.GetDirectoryName(file.Path)!);
                temporary = await folder.CreateFileAsync($".{file.Name}.{Guid.NewGuid():N}.tmp", CreationCollisionOption.GenerateUniqueName);
                await FileIO.WriteBytesAsync(temporary, bytes);
                await temporary.MoveAndReplaceAsync(file);
                temporary = null;
            });
            if (!result.IsSuccess)
            {
                AnnotationStatus.Text = result.Error!.Message;
                return false;
            }

            saved = true;
            AnnotationStatus.Text = "PDF saved. Annotations remain editable in this session.";
        }
        catch (Exception error)
        {
            AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
        }
        finally
        {
            try
            {
                if (temporary is not null) await temporary.DeleteAsync();
            }
            catch (Exception error)
            {
                AnnotationStatus.Text = _facade.SaveWriteFailure(error).Error!.Message;
            }
            finally
            {
                SetBusy(false);
            }
        }

        return saved;
    }

    /// <summary>
    /// Opens one of the samples that ship with the app, so a fresh install
    /// has something to render without the user supplying a PDF first, and a
    /// tester can reach the password prompt without hunting for an encrypted
    /// file. Goes through exactly the same open path as a picked file.
    /// </summary>
    private async void OpenSamplePlain_Click(object sender, RoutedEventArgs e) => await OpenSampleFileAsync(SamplePath, SampleDisplayName);

    private async void OpenSampleAes128_Click(object sender, RoutedEventArgs e) => await OpenSampleFileAsync(Aes128SamplePath, Aes128SampleDisplayName);

    private async void OpenSampleRc4128_Click(object sender, RoutedEventArgs e) => await OpenSampleFileAsync(Rc4128SamplePath, Rc4128SampleDisplayName);

    private async Task OpenSampleFileAsync(string path, string displayName)
    {
        SetBusy(true);
        byte[] bytes;
        try
        {
            bytes = await File.ReadAllBytesAsync(path);
        }
        catch (Exception error)
        {
            SetBusy(false);
            ReportFailedOpen(_facade.OpenReadFailure(error).Error!);
            return;
        }

        await OpenDocumentAsync(displayName, bytes);
    }

    /// <summary>
    /// The one open path both entry points share: open, retry on password,
    /// then either show the document or report the failure.
    /// </summary>
    private async Task OpenDocumentAsync(string displayName, byte[] bytes)
    {
        SetBusy(true);
        var result = await _facade.OpenAsync(new DocumentSource(displayName, bytes));
        SetBusy(false);

        // An encrypted document surfaces as a typed password failure rather
        // than a dead-end error: prompt for the password and retry instead of
        // stranding the user on the generic error state.
        if (!result.IsSuccess && result.Error!.RequiresPassword)
        {
            var unlocked = await OpenWithPasswordAsync(displayName, bytes);
            if (unlocked is null)
            {
                // The user dismissed the prompt; leave the current view as-is.
                return;
            }

            result = unlocked;
        }

        // Unsaved annotation work is not a dead end either: the guard exists to
        // make losing it a decision rather than an accident, so ask, then act
        // on the answer instead of leaving the reader to work it out.
        if (!result.IsSuccess && result.Error!.RequiresPendingEditDecision)
        {
            switch (await AskPendingEditDecisionAsync())
            {
                case PendingEditDecision.Cancel:
                    return;
                case PendingEditDecision.Save when !await SaveToPickedFileAsync():
                    // A cancelled picker or a failed write means the work they
                    // asked to keep is still unsaved; opening now would drop it.
                    return;
                case PendingEditDecision.Save:
                    result = await _facade.OpenAsync(new DocumentSource(displayName, bytes));
                    break;
                case PendingEditDecision.Discard:
                    result = await _facade.OpenAsync(new DocumentSource(displayName, bytes), discardPendingEdits: true);
                    break;
            }
        }

        if (!result.IsSuccess)
        {
            ReportFailedOpen(result.Error!);
            return;
        }

        ShowOpenedDocument(result.Value!);
    }

    private enum PendingEditDecision { Cancel, Save, Discard }

    /// <summary>
    /// Save, Discard or Cancel for the annotation work the open would replace.
    /// Cancel is both the close button and the dismissed-dialog default, so
    /// every way out that is not a deliberate choice keeps the work.
    /// </summary>
    private async Task<PendingEditDecision> AskPendingEditDecisionAsync()
    {
        var dialog = new ContentDialog
        {
            Title = "Unsaved annotation changes",
            Content = new TextBlock
            {
                Text = $"\"{_session?.DisplayName}\" has annotation changes that are not saved. Opening another document will discard them.",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = "Save",
            SecondaryButtonText = "Discard",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };

        return await dialog.ShowAsync() switch
        {
            ContentDialogResult.Primary => PendingEditDecision.Save,
            ContentDialogResult.Secondary => PendingEditDecision.Discard,
            _ => PendingEditDecision.Cancel,
        };
    }

    /// <summary>
    /// Reports an open that did not happen. The facade retires the current
    /// session only once a replacement has actually opened —
    /// <c>RetireCurrentSessionLocked</c> runs on the success path alone — so on
    /// any failure the document already on screen is still live and still
    /// editable, and the terminal error state would be a lie about it.
    ///
    /// The unsaved-changes guard is the case that makes this sharp: it asks the
    /// reader to save or undo first, and <see cref="ShowError"/> answered by
    /// hiding the pages, the pending edits, and Undo — every means of doing
    /// either. Only a failure with nothing left on screen is a dead end.
    /// </summary>
    private void ReportFailedOpen(UserSafeError error)
    {
        if (_session is null)
        {
            ShowError(error);
            return;
        }

        // SetBusy blanked the annotation toolbar on the way in and only
        // ShowOpenedDocument ever restores it, which this path never reaches.
        AnnotationStatus.Text = error.Message;
        UpdateAnnotationControls(_annotationState);
    }

    private void ShowOpenedDocument(DocumentSession session)
    {
        _stampPreviews.BeginSession(session.SessionId);
        _armedAnnotation = null;
        _selectedAnnotationId = null;
        _annotationState = null;
        _pointerDrag = null;
        _session = session;
        RefreshSessionCommands();
        DocumentTitle.Text = _session.DisplayName;
        ClearSearchResults();
        if (_session.State == DocumentSessionState.Empty)
        {
            ShowEmpty("This document has no pages.");
            return;
        }

        EmptyState.Visibility = Visibility.Collapsed;
        ErrorState.Visibility = Visibility.Collapsed;
        ShowDocumentPages(_session);
        _ = RefreshAnnotationStateAsync();
    }

    /// <summary>
    /// Prompts for the document password and retries opening until it
    /// succeeds, the user cancels, or a non-password failure occurs. Returns
    /// the successful (or terminally-failed) result, or null if the user
    /// dismissed the prompt without unlocking the document.
    /// </summary>
    private async Task<OperationResult<DocumentSession>?> OpenWithPasswordAsync(string displayName, byte[] bytes)
    {
        var passwordBox = new PasswordBox { PlaceholderText = "Password", PasswordRevealMode = PasswordRevealMode.Peek };
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
        EmptyStateMessage.Text = message;
        EmptyState.Visibility = Visibility.Visible;
        ErrorState.Visibility = Visibility.Collapsed;
        PageScroller.Visibility = Visibility.Collapsed;
        PageCounter.Text = "";
        ZoomLevel.Text = "";
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
        _isBusy = isBusy;
        BusyIndicator.IsActive = isBusy;
        BusyIndicator.Visibility = isBusy ? Visibility.Visible : Visibility.Collapsed;
        OpenButton.IsEnabled = !isBusy;
        OpenSampleButton.IsEnabled = !isBusy;
        PrintButton.IsEnabled = !isBusy;
        SearchButton.IsEnabled = !isBusy;
        ZoomInButton.IsEnabled = !isBusy;
        ZoomOutButton.IsEnabled = !isBusy;
        FitWidthButton.IsEnabled = !isBusy;
        FitPageButton.IsEnabled = !isBusy;
        RefreshSessionCommands();
        UpdateAnnotationControls(null);
    }

    /// <summary>
    /// Save needs both halves of its condition, and the two are set at
    /// different moments: <see cref="OpenDocumentAsync"/> clears busy while
    /// <c>_session</c> is still null, and only assigns it afterwards in
    /// <see cref="ShowOpenedDocument"/>. Deciding this inside
    /// <see cref="SetBusy"/> alone left Save disabled for the entire session,
    /// because nothing in the open path calls <see cref="SetBusy"/> again. The
    /// annotation toolbar escaped the same fate only because
    /// <see cref="RefreshAnnotationStateAsync"/> gives it a second pass — so
    /// this runs from both places instead of trusting one of them.
    /// </summary>
    private void RefreshSessionCommands()
    {
        SaveButton.IsEnabled = !_isBusy && _session is not null;
    }
}
