using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using System.Runtime.InteropServices.WindowsRuntime;

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

    private readonly PdfDocumentFacade _facade = new(new GeneratedPdfCore(), new FileDiagnosticLogger(FileDiagnosticLogger.DefaultPath), conservativeLifecycle: true);
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
        // Built here rather than in a field initializer because both of its
        // ports are this window: the write goes through the facade this
        // window owns, and the pause is this window's dispatcher timer.
        _pump = new ContentEditPump<ContentEditor>(new FacadeContentWriter(this), new DispatcherEditPause(_liveEdit));
        InitializeWindowLifecycle();
        InitializeWindowChrome();
    }

    private async void OpenButton_Click(object sender, RoutedEventArgs e) => await PickShellDocumentAsync();

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
        if (_isBusy || _dialogOpen || _shellPickingFile) return;
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
    /// The one open path every entry point shares: open, settle any pending
    /// edits, retry on password, then either show the document or report the
    /// failure.
    /// </summary>
    /// <remarks>
    /// The pending-edit decision has to come before the password prompt,
    /// because that is the order the facade refuses in: it checks the guard
    /// before the core ever sees the bytes, so while edits are pending an
    /// encrypted file can only answer "pending edits". Its password failure
    /// shows up on the retry after Save or Discard — and when the order here
    /// was the other way round, that retry fell straight through to
    /// <see cref="ReportFailedOpen"/> without ever prompting.
    /// </remarks>
    private async Task OpenDocumentAsync(string displayName, byte[] bytes)
    {
        SetBusy(true);
        if (!await PrepareDocumentLifecycleAsync())
        {
            SetBusy(false);
            RestoreAnnotationControls();
            return;
        }
        var result = await _facade.OpenAsync(new DocumentSource(displayName, bytes));
        SetBusy(false);

        // Unsaved annotation work is not a dead end: the guard exists to make
        // losing it a decision rather than an accident, so ask, then act on
        // the answer instead of leaving the reader to work it out.
        var discardPendingEdits = false;
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
                    discardPendingEdits = true;
                    result = await _facade.OpenAsync(new DocumentSource(displayName, bytes), discardPendingEdits: true);
                    break;
            }
        }

        // An encrypted document surfaces as a typed password failure rather
        // than a dead-end error: prompt for the password and retry instead of
        // stranding the user on the generic error state. A Discard has to
        // ride along on every attempt, or the guard would refuse each one.
        if (!result.IsSuccess && result.Error!.RequiresPassword)
        {
            var unlocked = await OpenWithPasswordAsync(displayName, bytes, discardPendingEdits);
            if (unlocked is null)
            {
                // The user dismissed the prompt; leave the current view as-is.
                // Nothing was discarded yet — the facade only retires the old
                // session once the new one opens — so the edits are still there.
                RestoreAnnotationControls();
                return;
            }

            result = unlocked;
        }

        if (!result.IsSuccess)
        {
            ReportFailedOpen(result.Error!);
            return;
        }

        ShowOpenedDocument(result.Value!);
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
        UpdatePageNavigationControls();
        try
        {
            return await dialog.ShowAsync();
        }
        finally
        {
            _dialogOpen = false;
            UpdatePageNavigationControls();
        }
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

}
