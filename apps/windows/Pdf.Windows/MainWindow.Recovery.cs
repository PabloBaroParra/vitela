using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Recovery;
using System.Diagnostics;
using System.Globalization;

namespace Pdf.Windows;

/// <summary>
/// Autosave and crash recovery for this window: the timer that keeps a
/// recovery copy of unsaved work, and the startup question that restores what
/// a crashed session left behind. The rules live in <c>Recovery/</c>; this
/// partial only wires them to the window.
/// </summary>
/// <remarks>
/// Disabled until <see cref="EnableCrashRecovery"/>, which only the production
/// startup calls — so the smoke harnesses, which build this window with their
/// own <c>App</c>, never touch the user's recovery directory.
/// </remarks>
public sealed partial class MainWindow
{
    private AutosaveScheduler? _autosave;
    private DispatcherQueueTimer? _autosaveTimer;
    private RecoveryDirectory? _recoveryDirectory;
    private bool _recoveryOffered;
    private readonly long _autosaveClockStart = Stopwatch.GetTimestamp();

    /// <summary>
    /// The file the open document was opened from or last saved to, which the
    /// Save picker suggests. <c>null</c> for a new, sample or shared-in document.
    /// </summary>
    private string? _documentOrigin;

    /// <summary>
    /// Starts autosave for this window and, once it has a root to show a dialog
    /// on, offers whatever a crashed session left in <paramref name="directory"/>.
    /// </summary>
    internal void EnableCrashRecovery(RecoveryDirectory directory)
    {
        if (_autosave is not null) return;
        _recoveryDirectory = directory;
        _autosave = new AutosaveScheduler(directory, new WindowAutosaveSource(this), LogRecovery, () =>
            AnnotationStatus.Text = "Autosave is unavailable, so a crash would lose your unsaved changes. Save to keep them.");
        _autosaveTimer = DispatcherQueue.CreateTimer();
        _autosaveTimer.Interval = AutosaveScheduler.Interval;
        _autosaveTimer.IsRepeating = true;
        _autosaveTimer.Tick += (_, _) => _ = TickAutosaveAsync();
        _autosaveTimer.Start();
        Closed += (_, _) => ShutDownAutosave();
        if (Content.XamlRoot is not null) OfferRecoveryOnce();
    }

    private static void LogRecovery(string line) => Trace.WriteLine(line);

    private TimeSpan AutosaveClock => Stopwatch.GetElapsedTime(_autosaveClockStart);

    private Task TickAutosaveAsync() => _autosave?.TickAsync(AutosaveClock) ?? Task.CompletedTask;

    /// <summary>
    /// The document's unsaved work was saved to disk, or the document holding it
    /// was replaced — after the unsaved-changes prompt, or because it had none.
    /// Either way its recovery entry is obsolete.
    /// </summary>
    private void ForgetRecoveryEntry() => _autosave?.Forget();

    /// <summary>
    /// A clean close: the close request already asked about unsaved work, and
    /// only Save or Discard lets it through. A crash runs none of this, which is
    /// what leaves the entry for the next launch.
    /// </summary>
    private void ShutDownAutosave()
    {
        _autosaveTimer?.Stop();
        _autosave?.Shutdown(AutosaveScheduler.ShutdownWait);
    }

    /// <summary>Called once the window has a XAML root; asks at most once per launch.</summary>
    private void OfferRecoveryOnce()
    {
        if (_recoveryOffered || _recoveryDirectory is not { } directory) return;
        _recoveryOffered = true;
        _ = OfferRecoveryAsync(directory);
    }

    private async Task OfferRecoveryAsync(RecoveryDirectory directory)
    {
        // Scanned before this window owns an entry — its slot opens on the first
        // write — so its own is never offered.
        IReadOnlyList<RecoveryOrphan> orphans;
        try
        {
            orphans = await Task.Run(() => directory.Orphans(LogRecovery));
        }
        catch (Exception error)
        {
            LogRecovery($"recovery: cannot scan {directory.Root}: {error.Message}");
            return;
        }
        await OfferAsync(new Queue<RecoveryOrphan>(orphans));
    }

    private enum RecoveryChoice { Restore, Discard, Later }

    /// <summary>
    /// One question per orphan, newest first. Restoring one ends the questions
    /// for this launch — the window holds one document — and the rest stay on
    /// disk for next time. Closing the dialog is not a decision: like the
    /// unsaved-changes prompt, only a deliberate choice loses work.
    /// </summary>
    private async Task OfferAsync(Queue<RecoveryOrphan> orphans)
    {
        while (orphans.TryDequeue(out var orphan))
        {
            switch (await AskRecoveryAsync(orphan.Meta))
            {
                case RecoveryChoice.Discard:
                    orphan.Discard();
                    continue;
                case RecoveryChoice.Later:
                    orphan.Dispose();
                    ReleaseAll(orphans);
                    AnnotationStatus.Text = "Your recovered work is kept for the next time Vitela starts.";
                    return;
                case RecoveryChoice.Restore:
                    if (await RestoreAsync(orphan))
                    {
                        // Dropping the claims releases their locks; the files stay.
                        ReleaseAll(orphans);
                        return;
                    }
                    continue;
            }
        }
    }

    private static void ReleaseAll(Queue<RecoveryOrphan> orphans)
    {
        while (orphans.TryDequeue(out var rest)) rest.Dispose();
    }

    private async Task<RecoveryChoice> AskRecoveryAsync(RecoveryMeta meta)
    {
        var dialog = new ContentDialog
        {
            Title = "Recover unsaved work?",
            Content = new TextBlock
            {
                Text = $"Vitela closed unexpectedly while “{meta.Name}” had unsaved changes. "
                    + $"A copy saved {SavedAtText(meta.SavedAt)} can be restored as an unsaved document."
                    + "\n\nUndo history is not recovered.",
                TextWrapping = TextWrapping.Wrap,
            },
            PrimaryButtonText = "Restore",
            SecondaryButtonText = "Discard",
            CloseButtonText = "Not now",
            DefaultButton = ContentDialogButton.Primary,
            XamlRoot = Content.XamlRoot,
        };
        return await ShowModalAsync(dialog) switch
        {
            ContentDialogResult.Primary => RecoveryChoice.Restore,
            ContentDialogResult.Secondary => RecoveryChoice.Discard,
            _ => RecoveryChoice.Later,
        };
    }

    /// <summary>When the copy was written, in the user's locale and time zone.</summary>
    private static string SavedAtText(long savedAt)
    {
        try
        {
            return DateTimeOffset.FromUnixTimeSeconds(savedAt).ToLocalTime().ToString("g", CultureInfo.CurrentCulture);
        }
        catch (ArgumentOutOfRangeException)
        {
            return savedAt.ToString(CultureInfo.InvariantCulture);
        }
    }

    /// <summary>
    /// Opens the orphan's bytes as an unsaved document named and placed like the
    /// one that crashed, then hands the entry to this window's autosave, which
    /// deletes it only once its own copy is written. A failure keeps the entry
    /// unless it proves the bytes can never be opened.
    /// </summary>
    /// <returns>Whether the document is now on screen.</returns>
    private async Task<bool> RestoreAsync(RecoveryOrphan orphan)
    {
        if (_isBusy) return Keep(orphan, "Vitela is busy. Your recovered work is kept for the next time it starts.");
        SetBusy(true);
        RecoveredOpen opened;
        try
        {
            byte[] bytes;
            try
            {
                bytes = await Task.Run(orphan.ReadPdf);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException or InvalidDataException)
            {
                if (!RecoveryOrphan.IsUnrecoverableReadFailure(error))
                {
                    LogRecovery($"recovery: keeping an entry that could not be read: {error.Message}");
                    return Keep(orphan, "The recovered copy could not be read. It is kept for the next time Vitela starts.");
                }
                LogRecovery($"recovery: discarding an entry that cannot be opened: {error.Message}");
                orphan.Discard();
                AnnotationStatus.Text = "The recovered copy could not be opened and was discarded.";
                return false;
            }
            opened = await _facade.OpenRecoveredAsync(new DocumentSource(orphan.Meta.Name, bytes), orphan.Meta.BreaksSignatures);
        }
        finally
        {
            SetBusy(false);
        }

        if (!opened.Result.IsSuccess)
        {
            RestoreAnnotationControls();
            if (opened.Unrecoverable)
            {
                LogRecovery($"recovery: discarding an entry that cannot be opened (reference {opened.Result.Error!.CorrelationId})");
                orphan.Discard();
                AnnotationStatus.Text = "The recovered copy could not be opened and was discarded.";
                return false;
            }
            LogRecovery($"recovery: keeping an entry that could not be opened (reference {opened.Result.Error!.CorrelationId})");
            return Keep(orphan, "The recovered copy could not be opened. It is kept for the next time Vitela starts.");
        }

        ShowOpenedDocument(opened.Result.Value!, orphan.Meta.Origin);
        AnnotationStatus.Text = "Recovered unsaved work. Save it to keep it.";
        _autosave?.Adopt(orphan);
        // Snapshot right away rather than a whole interval later, so the
        // crashed entry is replaced by this window's own as soon as possible.
        await TickAutosaveAsync();
        return true;
    }

    private bool Keep(RecoveryOrphan orphan, string status)
    {
        // Dropping the claim releases its lock; the files stay.
        orphan.Dispose();
        AnnotationStatus.Text = status;
        return false;
    }

    /// <summary>The window, as the autosave scheduler sees it.</summary>
    private sealed class WindowAutosaveSource(MainWindow window) : IAutosaveSource
    {
        public Task<RecoveryObservation?> ObserveAsync() => window._facade.ObserveForRecoveryAsync();

        public bool EditingText => window._pump.Box is not null;

        public Task<OperationResult<RecoverySnapshot>> SnapshotAsync(string sessionId) => window._facade.RecoverySnapshotAsync(sessionId);

        public (string Name, string? Origin) Describe() => (window._session?.DisplayName ?? "document.pdf", window._documentOrigin);
    }
}
