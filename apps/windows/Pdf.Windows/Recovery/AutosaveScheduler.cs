using Pdf.Windows.Facade;

namespace Pdf.Windows.Recovery;

/// <summary>What the scheduler needs from the window it protects.</summary>
public interface IAutosaveSource
{
    /// <summary>The open document as the facade sees it, or <c>null</c> when none is open.</summary>
    Task<RecoveryObservation?> ObserveAsync();

    /// <summary>Whether the inline retype box is open; see <see cref="AutosavePolicy.Decide"/>.</summary>
    bool EditingText { get; }

    /// <summary>The bytes a Save of <paramref name="sessionId"/> would write, serialized off the UI thread.</summary>
    Task<OperationResult<RecoverySnapshot>> SnapshotAsync(string sessionId);

    /// <summary>The document's display name and the file it came from, for the sidecar.</summary>
    (string Name, string? Origin) Describe();
}

/// <summary>
/// One window's autosave: every <see cref="Interval"/>, a document with unsaved
/// changes is written to the window's own recovery entry, and a document with
/// none deletes it.
/// </summary>
/// <remarks>
/// <para>
/// The bytes are the ones a real Save would write, serialized by the facade
/// off the UI thread; the disk write runs on the thread pool too. A forget —
/// a save, a replaced document, a clean close — bumps an epoch the worker
/// checks both before and after it writes, so a write that started before the
/// forget cannot resurrect the entry.
/// </para>
/// <para>
/// Only document bytes are recovered, never the undo history: the core has no
/// serialization of the edit log. Meant to be driven from one thread — the
/// window's dispatcher — like the rest of the shell.
/// </para>
/// </remarks>
public sealed class AutosaveScheduler(RecoveryDirectory? directory, IAutosaveSource source, Action<string> log, Action notifyUnavailable)
{
    /// <summary>
    /// How often a window looks for unsaved changes to write. A crash loses at
    /// most this much work; a shorter interval would re-serialize a large
    /// document more often while the user is still editing it.
    /// </summary>
    public static readonly TimeSpan Interval = TimeSpan.FromSeconds(60);

    /// <summary>How long a clean exit waits for a snapshot still being written.</summary>
    public static readonly TimeSpan ShutdownWait = TimeSpan.FromSeconds(5);

    private EntryKey? _key;
    private RecoverySlot? _slot;
    private RecoveryRevision? _written;
    private AutosaveBackoff _backoff = new();
    private RecoveryOrphan? _adopted;
    private Worker? _worker;
    private long _epoch;
    private bool _ticking;
    private bool _tickAgain;
    private bool _stopped;

    /// <summary>Runs just before the worker writes to disk, after its last forget check. Tests only.</summary>
    internal Action? BeforeDiskWrite { get; set; }

    /// <summary>How many snapshots have been serialized — what tests read to tell a skipped attempt from a made one.</summary>
    internal int Attempts { get; private set; }

    /// <summary>The revision the entry on disk holds, if any.</summary>
    internal RecoveryRevision? Written => _written;

    /// <summary>One look at the document: write it, forget its entry, or wait.</summary>
    /// <param name="now">A monotonic clock reading, for the failure backoff.</param>
    public async Task TickAsync(TimeSpan now)
    {
        if (directory is null || _stopped) return;
        // A tick asked for while one runs — an adopt during the timer's own —
        // runs once that one is done, rather than racing it or being lost.
        if (_ticking)
        {
            _tickAgain = true;
            return;
        }
        _ticking = true;
        try
        {
            do
            {
                _tickAgain = false;
                await TickOnceAsync(directory, now);
            }
            while (_tickAgain && !_stopped);
        }
        finally
        {
            _ticking = false;
        }
    }

    private async Task TickOnceAsync(RecoveryDirectory directory, TimeSpan now)
    {
        var started = Interlocked.Read(ref _epoch);
        var observed = await source.ObserveAsync();
        if (_stopped || Forgotten(started)) return;
        switch (AutosavePolicy.Decide(observed, source.EditingText, _written))
        {
            case AutosaveDecision.Forget:
                Forget();
                return;
            case AutosaveDecision.Wait:
                return;
        }
        // Backing off after a failure: not even serialized, since the attempt
        // would most likely fail the same way.
        if (!_backoff.Ready(now)) return;

        Attempts++;
        var snapshot = await source.SnapshotAsync(observed!.SessionId);
        if (_stopped || Forgotten(started)) return;
        if (!snapshot.IsSuccess)
        {
            Failed(snapshot.Error!.Message, now);
            return;
        }
        var (name, origin) = source.Describe();
        var meta = new RecoveryMeta(DateTimeOffset.UtcNow.ToUnixTimeSeconds(), name, origin, snapshot.Value!.BreaksSignatures);
        if (_slot is null)
        {
            _key ??= EntryKey.Fresh();
            try
            {
                _slot = directory.OpenSlot(_key);
            }
            catch (Exception error) when (error is IOException or UnauthorizedAccessException)
            {
                Failed($"cannot open {directory.Root}: {error.Message}", now);
                return;
            }
        }

        var paths = _slot.Paths;
        var bytes = snapshot.Value.Bytes;
        var beforeDiskWrite = BeforeDiskWrite;
        var worker = new Worker();
        worker.Job = Task.Run(() => worker.Run(paths, bytes, meta, () => Forgotten(started), beforeDiskWrite));
        _worker = worker;
        bool landed;
        try
        {
            landed = await worker.Job;
        }
        catch (Exception error)
        {
            Failed(error.Message, now);
            return;
        }
        // Forgotten while writing: whoever forgot it deleted it.
        if (!landed || Forgotten(started)) return;
        _written = new RecoveryRevision(snapshot.Value.SessionId, snapshot.Value.EditRevision);
        _backoff.Succeeded();
        // A restored document is now safely in this window's own entry.
        var adopted = _adopted;
        _adopted = null;
        adopted?.Discard();
    }

    /// <summary>
    /// Deletes this window's entry, and any restored one it was keeping: the
    /// document it protected was saved, closed, discarded or replaced. Ends any
    /// failure streak too, so the next document is written — and its own
    /// failures reported — without waiting out the last one's backoff.
    /// </summary>
    public void Forget()
    {
        Interlocked.Increment(ref _epoch);
        _written = null;
        _backoff = new AutosaveBackoff();
        var slot = _slot;
        _slot = null;
        slot?.Clear();
        var adopted = _adopted;
        _adopted = null;
        adopted?.Discard();
    }

    /// <summary>
    /// Takes over a crashed session's entry once its document is on screen. It
    /// stays on disk, and locked, until this window has written the same
    /// document to its own entry, so a crash in between still has something to
    /// recover. The caller ticks right after, rather than a whole interval later.
    /// </summary>
    public void Adopt(RecoveryOrphan orphan)
    {
        var previous = _adopted;
        _adopted = orphan;
        previous?.Discard();
    }

    /// <summary>
    /// The window is going away cleanly: stop, and delete the entry.
    /// </summary>
    /// <remarks>
    /// The forget alone is not enough when a snapshot is being written: the
    /// worker checks the epoch again only once its files are in place, and a
    /// process that exits before then leaves a whole entry behind for the next
    /// launch to offer. So the exit waits for it — bounded, because a hung disk
    /// must not keep the application from quitting, and the worst a timeout
    /// costs is that offer. A worker that has not reached the disk yet is not
    /// waited for: the forget lands before its last check, so it writes nothing.
    /// </remarks>
    public void Shutdown(TimeSpan wait)
    {
        _stopped = true;
        var worker = _worker;
        Forget();
        if (worker is null || !worker.MayBeWriting || worker.Job is not { IsCompleted: false } job) return;
        try
        {
            if (!job.Wait(wait))
            {
                log($"recovery: an autosave was still being written after {wait}; its entry may be offered at the next launch");
            }
        }
        catch (AggregateException)
        {
            // A failed write left nothing to clean up that the forget did not.
        }
    }

    /// <summary>
    /// Lets go of the entry the way a crash does: the lock is released and the
    /// files stay. For tests and the recovery smoke; a running window never
    /// calls it.
    /// </summary>
    internal void ReleaseLikeACrash()
    {
        _stopped = true;
        _slot?.ReleaseLikeACrash();
        _slot = null;
        _adopted?.Dispose();
        _adopted = null;
        _written = null;
    }

    private bool Forgotten(long started) => Interlocked.Read(ref _epoch) != started;

    /// <summary>
    /// A snapshot could not be written. The first failure of a streak tells the
    /// user — once, not in a dialog: their work is still open in front of them,
    /// and only a crash would cost it.
    /// </summary>
    private void Failed(string error, TimeSpan now)
    {
        log($"recovery: autosave failed: {error}");
        if (_backoff.Failed(now)) notifyUnavailable();
    }

    /// <summary>One snapshot's disk write, as a clean exit sees it.</summary>
    private sealed class Worker
    {
        private int _writing;

        public Task<bool>? Job { get; set; }

        /// <summary>Set before the worker's last forget check: from then on it may put files on disk.</summary>
        public bool MayBeWriting => Volatile.Read(ref _writing) != 0;

        /// <summary><c>true</c> when the entry now holds the snapshot; <c>false</c> when it was forgotten meanwhile and holds nothing.</summary>
        public bool Run(EntryPaths paths, byte[] bytes, RecoveryMeta meta, Func<bool> forgotten, Action? beforeDiskWrite)
        {
            // A full fence, then the check: a shutdown that forgets and then
            // finds this unset is ordered before it, so the check sees the forget
            // and nothing is written.
            Interlocked.Exchange(ref _writing, 1);
            if (forgotten()) return false;
            beforeDiskWrite?.Invoke();
            paths.Write(bytes, meta);
            // Checked here, on the worker, and not only once the result reaches
            // the dispatcher: a window closed mid-write may never run that
            // continuation, and files it should have deleted would be offered
            // for recovery at the next launch.
            if (forgotten())
            {
                paths.RemoveData();
                return false;
            }
            return true;
        }
    }
}
