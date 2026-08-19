using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
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

    /// <summary>
    /// Whether a <see cref="ContentDialog"/> is on screen right now, so
    /// <see cref="ShowModalAsync"/> never starts a second one — WinUI throws
    /// on a concurrent <c>ShowAsync</c>, and the three flows that prompt are
    /// all <c>async void</c>, where that throw is an unhandled exception
    /// rather than a failed operation.
    /// </summary>
    /// <remarks>
    /// A guard, not a fix for an observed bug. The suspicion was that the
    /// accelerators reach the prompts behind an open dialog — they carry no
    /// <c>ScopeOwner</c>, and <see cref="_isBusy"/> is already false by then,
    /// since every prompt is raised after <c>SetBusy(false)</c> precisely so
    /// the window stays responsive while the reader decides. Measured instead
    /// (2026-08-19, x64 Debug): with the pending-edit dialog up, Ctrl+N and
    /// Ctrl+O do nothing at all, while the same Ctrl+O with no dialog open
    /// raises the file picker — so WinUI blocks main-tree accelerators for the
    /// duration. No reachable crash path is known today.
    ///
    /// It stays because nothing else in the shell enforces one-dialog-at-a-
    /// time, and the flows chain: the pending-edit Save branch runs
    /// <see cref="SaveToPickedFileAsync"/>, which can raise a second prompt of
    /// its own. Do not weaken it into an assertion of the behaviour above.
    /// </remarks>
    private bool _dialogOpen;

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
    /// <remarks>
    /// The wrapper exists for the <c>finally</c>. A save never replaces the
    /// document — it is still open and still editable on every path out,
    /// including the one that says so out loud ("Annotations remain editable
    /// in this session") — so the annotation toolbar
    /// <see cref="SetBusy"/> blanked has to come back no matter how this
    /// returns. Restoring at each <c>return</c> instead would be five call
    /// sites to keep in step, and missing one is precisely the bug this
    /// fixes.
    /// </remarks>
    private async Task<bool> SaveToPickedFileAsync()
    {
        try
        {
            return await PickDestinationAndWriteAsync();
        }
        finally
        {
            RestoreAnnotationControls();
        }
    }

    private async Task<bool> PickDestinationAndWriteAsync()
    {
        if (_session is null) return false;

        // Before the picker, not after: the answer does not depend on where
        // the file goes, so asking first means a reader who backs out never
        // had to choose a destination for a save that was never going to
        // happen.
        var acknowledged = await AskSignatureLossAsync();
        if (acknowledged is null) return false;

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
            }, acknowledged.Value);
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
                RestoreAnnotationControls();
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
                    RestoreAnnotationControls();
                    return;
                case PendingEditDecision.Save when !await SaveToPickedFileAsync():
                    // A cancelled picker or a failed write means the work they
                    // asked to keep is still unsaved; opening now would drop it.
                    RestoreAnnotationControls();
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

    /// <summary>
    /// Ctrl+N replaces the open document with a new blank one. It reuses the
    /// same pending-edit guard as opening a file, because it replaces the
    /// current document the same way — the only thing missing is a file to
    /// read, so <see cref="PdfDocumentFacade.CreateBlankAsync"/> stands in for
    /// <see cref="PdfDocumentFacade.OpenAsync"/> and the rest of the flow
    /// (Save/Discard/Cancel, then <see cref="ShowOpenedDocument"/>) is
    /// unchanged. There is no toolbar button for it, matching the GTK shell:
    /// it exists as a shortcut only.
    /// </summary>
    /// <remarks>
    /// The A4 page the reader lands on comes from the core's
    /// <c>create_document_with_blank_page</c>, added for this. Its sibling
    /// <c>create_blank_document</c> is the wrong call here — it returns a
    /// ZERO-page document, taking the page size and orientation as the default
    /// for pages inserted later, which left this shell showing "This document
    /// has no pages." with every annotation tool disabled and no way to add
    /// one, since it exposes no page insertion at all.
    ///
    /// Routing the first page through <c>apply_edit(InsertBlankPage)</c> — the
    /// only page-structural command that reaches a shell — would not have
    /// fixed it: <c>apply_edit</c> mutates the document model without
    /// rebuilding the render handle a zero-page document never had, so the
    /// page count would read 1 while rendering still failed, and the new
    /// document would carry an unsaved edit into the guard above. The page has
    /// to exist before the handle does, which is what the new entrypoint (and
    /// the GTK shell's <c>new_blank_document</c>) does.
    /// </remarks>
    private async void NewDocument_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        args.Handled = true;
        if (_isBusy) return;

        SetBusy(true);
        var result = await _facade.CreateBlankAsync();
        SetBusy(false);

        if (!result.IsSuccess && result.Error!.RequiresPendingEditDecision)
        {
            switch (await AskPendingEditDecisionAsync())
            {
                case PendingEditDecision.Cancel:
                    RestoreAnnotationControls();
                    return;
                case PendingEditDecision.Save when !await SaveToPickedFileAsync():
                    // A cancelled picker or a failed write means the work they
                    // asked to keep is still unsaved; creating a new document
                    // now would drop it.
                    RestoreAnnotationControls();
                    return;
                case PendingEditDecision.Save:
                    result = await _facade.CreateBlankAsync();
                    break;
                case PendingEditDecision.Discard:
                    result = await _facade.CreateBlankAsync(discardPendingEdits: true);
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

    /// <summary>
    /// Asks about a signature the save would break, if there is one.
    /// </summary>
    /// <returns>
    /// The acknowledgement to save with, or <c>null</c> to abandon the save.
    /// <c>false</c> is the ordinary case — nothing to acknowledge — and
    /// <c>true</c> means the reader was shown what they would lose and chose
    /// to continue.
    /// </returns>
    /// <remarks>
    /// There is no third option to offer: a save rewrites the file, and the
    /// signature covers the bytes being replaced, so it cannot be carried
    /// over. The dialog exists to make that a decision instead of a
    /// discovery. Cancel is the default and the dismissed-dialog result, so
    /// every way out that is not deliberate keeps the signed file intact —
    /// the same rule the unsaved-changes prompt follows.
    ///
    /// The text says the signature is <em>invalidated</em>, not removed,
    /// because that is what happens: nothing in <c>pdf-save</c>'s full
    /// rewrite strips <c>/Sig</c>, <c>/FT /Sig</c> or
    /// <c>/AcroForm /SigFlags</c>, so the saved file still carries the
    /// signature and a reader opening it reports it as invalid rather than
    /// absent. For someone about to send the file on, an unsigned document
    /// and one that looks tampered with are not the same choice. The GTK
    /// shell's <c>confirm_signature_loss</c> says this in the same words —
    /// keep them together.
    ///
    /// It names no file, deliberately. The GTK shell cannot always supply one
    /// (an embedded sample or a new blank document has no path), and one
    /// prompt in two voices is worse than one prompt with no filename. It
    /// also removes the only reason this method reached for
    /// <c>_session</c> after an await, where a re-entrant open could have
    /// left the reader looking at an empty pair of quotes.
    ///
    /// A failure to even determine this is reported and treated as cancel:
    /// saving anyway would risk breaking a signature nobody confirmed, and
    /// silently downgrading to "no signature" is how this went wrong before.
    /// </remarks>
    private async Task<bool?> AskSignatureLossAsync()
    {
        // Busy only around the query — the scan can take a moment on a large
        // document — and never around the dialog, which needs a live window
        // to be answered in.
        SetBusy(true);
        var query = await _facade.WillInvalidateSignaturesAsync(_session!.SessionId);
        SetBusy(false);

        if (!query.IsSuccess)
        {
            AnnotationStatus.Text = query.Error!.Message;
            return null;
        }

        if (!query.Value) return false;

        var dialog = new ContentDialog
        {
            Title = "Saving will break this document's signature",
            Content = new TextBlock
            {
                Text = "This document is signed. Saving rewrites the file, so the signature will no longer match what it covers."
                    + "\n\nIt is not removed: the saved file still carries the signature, and PDF readers will report it as invalid rather than missing."
                    + "\n\nTo keep a copy that still verifies, cancel and save to a different file.",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = "Save anyway",
            CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close,
            XamlRoot = Content.XamlRoot,
        };

        if (await ShowModalAsync(dialog) != ContentDialogResult.Primary) return null;
        return true;
    }

    /// <summary>
    /// Shows a dialog, or answers <see cref="ContentDialogResult.None"/> when
    /// one is already up, so a second prompt can never reach WinUI's "only a
    /// single ContentDialog can be open at any time". See
    /// <see cref="_dialogOpen"/> for what this is and is not evidence of.
    /// </summary>
    /// <remarks>
    /// <c>None</c> costs the callers nothing to honour: all three already read
    /// "not Primary" as the dismissed case — abandon the save, keep the
    /// pending edits, leave the document alone — so refusing degrades to the
    /// choice that changes least, which is the rule the prompts follow
    /// anyway.
    ///
    /// A flag rather than disabling the accelerators, because there is no one
    /// place to re-enable them from: the prompts come from three flows, and
    /// <see cref="OpenWithPasswordAsync"/> re-shows the same dialog in a loop.
    /// Releasing in <c>finally</c> keeps the flag honest even if a dialog
    /// throws.
    /// </remarks>
    private async Task<ContentDialogResult> ShowModalAsync(ContentDialog dialog)
    {
        if (_dialogOpen) return ContentDialogResult.None;

        _dialogOpen = true;
        try
        {
            return await dialog.ShowAsync();
        }
        finally
        {
            _dialogOpen = false;
        }
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

        return await ShowModalAsync(dialog) switch
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

        AnnotationStatus.Text = error.Message;
        RestoreAnnotationControls();
    }

    /// <summary>
    /// Hands the annotation toolbar back to the document still on screen.
    ///
    /// <see cref="SetBusy"/> blanks it on both edges — it calls
    /// <c>UpdateAnnotationControls(null)</c> going in *and* coming out — and
    /// only <see cref="ShowOpenedDocument"/> ever restores it. So every way of
    /// leaving an open or a create without a new document owes the reader this
    /// call, or they are left looking at their own document with Highlight,
    /// Delete and — worst of it — Undo greyed out, which is the one control
    /// that would clear the pending edits the prompt was asking about.
    /// </summary>
    private void RestoreAnnotationControls() => UpdateAnnotationControls(_annotationState);

    private void ShowOpenedDocument(DocumentSession session)
    {
        _stampPreviews.BeginSession(session.SessionId);
        _armedAnnotation = null;
        _selectedAnnotationId = null;
        _annotationState = null;
        _pointerDrag = null;
        ResetSelectionState();
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
            if (await ShowModalAsync(dialog) != ContentDialogResult.Primary)
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
