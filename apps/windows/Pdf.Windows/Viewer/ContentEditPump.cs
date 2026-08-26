namespace Pdf.Windows.Viewer;

/// <summary>
/// One inline editor, as the write pump sees it.
/// </summary>
/// <remarks>
/// Deliberately says nothing about text boxes or canvases. The pump decides
/// <em>when</em> a run's text reaches the document and <em>whether</em> the
/// editor may close; drawing the box, dressing it in the page's font and
/// taking it off again is the shell's, and it is the half that needs a UI
/// runtime. Keeping the two apart is what lets the ordering rules below be
/// exercised without one.
/// </remarks>
public interface IContentEditBox
{
    uint PageIndex { get; }

    ulong RunId { get; }

    /// <summary>The run's text as the opened file holds it.</summary>
    string RunText { get; }

    /// <summary>
    /// What the box showed when it opened — the page's own text, or the text
    /// an earlier unsaved edit gave it. Abandoning restores exactly this,
    /// which is only knowable at open time: by the time it is asked for, the
    /// document has already been written to.
    /// </summary>
    string OpenedWith { get; }

    /// <summary>What the box says now.</summary>
    string Text { get; set; }

    /// <summary>Takes the box off the page. Called at most once per box.</summary>
    void Close();
}

/// <summary>What became of one write.</summary>
public enum ContentWriteOutcome
{
    /// <summary>Recorded. The document holds this text for the run now.</summary>
    Written,

    /// <summary>
    /// Understood and declined — a character the run's font cannot encode is
    /// the case that matters. The reader clears it by typing something else,
    /// so the pump stops until the text changes rather than re-sending.
    /// </summary>
    Refused,

    /// <summary>
    /// Nothing happened and nothing will: the document this targeted is gone.
    /// Distinct from <see cref="Refused"/> on purpose — latching a refusal
    /// here would strand a later editor on a failure that was never the
    /// reader's.
    /// </summary>
    Abandoned,
}

/// <summary>Where a pump's text goes.</summary>
public interface IContentEditWriter<in TBox>
    where TBox : IContentEditBox
{
    /// <summary>
    /// Records <paramref name="text"/> against the run <paramref name="box"/>
    /// is editing. Whoever implements this tells the reader what happened;
    /// the pump only needs to know whether to keep going.
    /// </summary>
    Task<ContentWriteOutcome> WriteAsync(TBox box, string text);
}

/// <summary>
/// The pause in typing that separates one write from the next.
/// </summary>
/// <remarks>
/// A port rather than a timer, because scheduling is the shell's business —
/// on Windows it is a <c>DispatcherTimer</c> — while stopping it is part of
/// every rule the pump enforces. A commit that left the pause running would
/// fire a write into an editor that has already closed.
/// </remarks>
public interface IEditPause
{
    /// <summary>Cancels a pending pass without starting another.</summary>
    void Stop();

    /// <summary>Restarts the pause: the next pass is one full pause from now.</summary>
    void Restart();
}

/// <summary>What abandoning an edit left for the shell to do.</summary>
public enum ContentEditAbandon
{
    /// <summary>Nothing reached the document, so nothing has to be taken back.</summary>
    Nothing,

    /// <summary>
    /// The box opened on the page's own text, so the queued command is this
    /// session's and undo removes it — the document goes back to having no
    /// edit for this run at all.
    /// </summary>
    Undo,

    /// <summary>
    /// The box opened on text an earlier edit had put there. Undo would throw
    /// that edit away too, so the way back was to write the earlier text
    /// again, which has already happened.
    /// </summary>
    Rewritten,
}

/// <summary>
/// Gets what the reader types into the document, and decides when the inline
/// editor may close.
/// </summary>
/// <remarks>
/// The document keeps up with the keyboard rather than waiting for Enter.
/// That is affordable for one reason: the core <em>amends</em> the command
/// already queued for a run instead of appending one per keystroke, so a
/// whole typing session stays one entry in the edit log and one step of undo.
///
/// Everything here is ordering, and ordering is why it is its own class. The
/// shell drives it from a single thread — the UI dispatcher — where
/// continuations run in the order they were registered, and three of the four
/// rules below exist because of that order:
///
/// <list type="bullet">
/// <item>opening an editor closes the one that was open, because two of these
/// can be started for one double click and only one can be current;</item>
/// <item>waiting for writes <em>drains</em> rather than awaiting once, because
/// the loop that owns a write registered its continuation first and has
/// already started the next one by the time anyone else wakes up;</item>
/// <item>a history step settles the editor before the log moves, because a box
/// showing text the document no longer has would write it straight back;</item>
/// <item>a refused write leaves the box open, because typing something else is
/// the only way the reader can clear it.</item>
/// </list>
/// </remarks>
public sealed class ContentEditPump<TBox>(IContentEditWriter<TBox> writer, IEditPause pause)
    where TBox : class, IContentEditBox
{
    /// <summary>
    /// Text written against a run but not yet re-read from the file, keyed by
    /// page and run id.
    /// </summary>
    /// <remarks>
    /// The parse the shell holds describes the document as opened, and a
    /// pending edit never changes it — that is deliberate, since the ids in it
    /// are what the core matches an edit against. But the reader sees the new
    /// words on the page, so re-opening the editor on that run has to show
    /// them too, not the text they replaced.
    /// </remarks>
    private readonly Dictionary<(uint Page, ulong Run), string> _written = [];

    private TBox? _box;
    private Task? _inFlight;
    private bool _refused;

    /// <summary>The editor currently open, or <c>null</c>.</summary>
    public TBox? Box => _box;

    /// <summary>Whether the last write was declined and the pump is waiting for different text.</summary>
    public bool Refused => _refused;

    /// <summary>Text this session has written for a run, or <c>null</c> if none.</summary>
    public string? WrittenFor(uint page, ulong run) =>
        _written.TryGetValue((page, run), out var text) ? text : null;

    /// <summary>
    /// Makes <paramref name="box"/> the live editor, closing whatever was live
    /// before.
    /// </summary>
    /// <remarks>
    /// The close is the point. A shell opens these from a pointer handler that
    /// cannot await, so two opens overlap whenever the first one parks — on a
    /// page's first content load, or on a commit still draining. Assigning
    /// <see cref="Box"/> is what makes an editor current, and one that stops
    /// being current while its box is still on the page is an orphan nothing
    /// owns: still visible, still focusable, still carrying its handlers.
    /// </remarks>
    public void Open(TBox box)
    {
        Close();
        _box = box;
    }

    /// <summary>Closes the live editor, if any, without recording anything.</summary>
    public void Close()
    {
        pause.Stop();
        _refused = false;
        // Cleared before the box is told, so anything the close runs into
        // finds no live editor rather than the one being dismantled.
        var closing = _box;
        _box = null;
        closing?.Close();
    }

    /// <summary>
    /// Whatever was refused is being retyped, so let the pump try again.
    /// </summary>
    public void Retry() => _refused = false;

    /// <summary>The box changed: write once typing pauses.</summary>
    public void Schedule() => pause.Restart();

    /// <summary>
    /// Forgets which runs carry an unsaved write, after an undo or redo.
    /// </summary>
    /// <remarks>
    /// Nothing here knows which command a history step moved, so the honest
    /// answer is none of them: the next editor prefills from the parsed page
    /// rather than from text that may no longer be queued.
    /// </remarks>
    public void Forget() => _written.Clear();

    /// <summary>Drops everything, for a new document replacing the old one.</summary>
    public void Reset()
    {
        Close();
        _written.Clear();
    }

    /// <summary>
    /// Writes the editor's text to the document, one write at a time.
    /// </summary>
    /// <remarks>
    /// Keystrokes arriving during a write are not queued behind it: the loop
    /// re-reads the box afterwards and writes whatever it says now. Only the
    /// latest text has any meaning, since each write replaces the last rather
    /// than stacking on it.
    /// </remarks>
    public async Task PumpAsync()
    {
        if (_inFlight is not null)
        {
            return;
        }

        while (_box is { } box && !_refused)
        {
            var text = box.Text;
            if (text == InDocumentFor(box))
            {
                return;
            }

            var write = SendAsync(box, text);
            _inFlight = write;
            try
            {
                await write;
            }
            finally
            {
                _inFlight = null;
            }
        }
    }

    /// <summary>
    /// Waits until no write is in flight and none is about to start.
    /// </summary>
    /// <remarks>
    /// Awaiting the in-flight write once is not enough, and the reason is
    /// ordering. <see cref="PumpAsync"/> awaits that same task and registered
    /// its continuation first, so when a caller here resumes, the pump has
    /// already run: it cleared the slot, re-read the box, found the keystrokes
    /// that arrived during the last write and started another one. A caller
    /// acting on that moment would be acting while a write it never saw was
    /// still running — and <see cref="PumpAsync"/> cannot warn it, because
    /// that write is exactly what makes the pump no-op on its own guard.
    ///
    /// So this drains: each pass awaits a distinct write, and it ends when the
    /// document has caught up with the box, or when a refusal stops the pump
    /// from starting anything more.
    /// </remarks>
    public async Task DrainAsync()
    {
        while (_inFlight is { } inFlight)
        {
            await inFlight;
        }
    }

    /// <summary>
    /// Resolves the open editor: makes sure what it says has reached the
    /// document, then closes it. A click on another run, leaving the mode, and
    /// pressing Enter all arrive here.
    /// </summary>
    public async Task CommitAsync()
    {
        pause.Stop();
        await DrainAsync();
        await PumpAsync();

        // Closed only once every write has landed and none was refused. Enter
        // on text the run's font cannot encode has to leave the box open: the
        // reader clears that failure by typing something else, and a refusal
        // arriving after the box is gone is one they have no way to act on.
        if (_box is not null && !_refused)
        {
            Close();
        }
    }

    /// <summary>
    /// Puts the run back the way the editor found it, then closes the box, and
    /// reports what the shell still owes.
    /// </summary>
    /// <remarks>
    /// Escape has real work to do now. When the document only changed on
    /// Enter, abandoning an edit meant closing a box; with the document
    /// keeping up with the keyboard, the text the reader is walking away from
    /// is already in it.
    ///
    /// Two ways back, and which one applies is decided by whether this session
    /// <em>created</em> the queued command or amended one that was already
    /// there. Both rely on the queued command for this run being the last
    /// entry in the log, which holds while an editor is open: every other way
    /// of editing goes through a page click or the toolbar, and both settle
    /// the editor before they run.
    /// </remarks>
    public async Task<ContentEditAbandon> AbandonAsync()
    {
        pause.Stop();
        // Drained, not merely awaited once: a write still running would land
        // after the undo the caller is about to run and re-queue the very
        // command that undo removed. The reader would press Escape and keep
        // the edit.
        await DrainAsync();

        if (_box is not { } box)
        {
            return ContentEditAbandon.Nothing;
        }

        if (InDocumentFor(box) == box.OpenedWith)
        {
            Close();
            return ContentEditAbandon.Nothing;
        }

        if (box.OpenedWith == box.RunText)
        {
            _written.Remove((box.PageIndex, box.RunId));
            Close();
            return ContentEditAbandon.Undo;
        }

        box.Text = box.OpenedWith;
        await SendAsync(box, box.OpenedWith);
        Close();
        return ContentEditAbandon.Rewritten;
    }

    /// <summary>
    /// Settles the inline editor before the edit log itself moves.
    /// </summary>
    /// <remarks>
    /// Undo and redo rewrite what the document holds for a run, and an open
    /// box does not follow them: it goes on showing text the document no
    /// longer has. That is not merely stale. The next commit compares the box
    /// against the restored text, sees a difference, and writes the undone
    /// edit straight back — and because the core records that as a fresh
    /// command, the redo stack is cleared with it, so the step the reader
    /// asked for ends up both reversed and unreachable. The box therefore goes
    /// before the history moves.
    ///
    /// Drained first, and only then closed. Draining is what commits the
    /// keystrokes already on their way, so the entry the history step lands on
    /// is the whole typing session rather than half of it — the core amends a
    /// run's queued command instead of stacking one per keystroke, which is
    /// what makes that a single step to begin with. A write left running would
    /// instead land after the step and undo it.
    /// </remarks>
    public async Task SettleForHistoryAsync()
    {
        pause.Stop();
        await DrainAsync();
        Close();
    }

    /// <summary>The text the document currently holds for a box's run.</summary>
    private string InDocumentFor(TBox box) =>
        WrittenFor(box.PageIndex, box.RunId) ?? box.RunText;

    private async Task SendAsync(TBox box, string text)
    {
        switch (await writer.WriteAsync(box, text))
        {
            case ContentWriteOutcome.Written:
                _written[(box.PageIndex, box.RunId)] = text;
                break;
            case ContentWriteOutcome.Refused:
                // Latched until the text changes again: without this, the pump
                // would re-send the same rejected text on every pass.
                _refused = true;
                break;
            default:
                // The document is gone. Ending the loop is the whole job — the
                // shell's own reset has already taken the box off a page that
                // no longer exists, so there is nothing here to close.
                _box = null;
                break;
        }
    }
}
