using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;

namespace Pdf.Windows;

/// <summary>
/// Getting what the reader types into the document — the half of content-edit
/// mode that talks to the core.
///
/// The document keeps up with the keyboard rather than waiting for Enter. That
/// is affordable for one reason: the core <em>amends</em> the command already
/// queued for a run instead of appending one per keystroke, so a whole typing
/// session stays one entry in the edit log and one step of undo. Re-rendering
/// the single page it changed costs about ten milliseconds, so what the reader
/// sees stays a fraction of a second behind what they typed.
///
/// Split from <c>MainWindow.ContentEditor.cs</c>, which owns the box itself:
/// this owns when a write happens, that owns what the reader is looking at.
/// The same seam the GTK shell draws between <c>content_edit::editor</c> and
/// <c>content_edit::command</c>.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>The write in flight, so a second one waits for it instead of racing it.</summary>
    private Task? _contentCommit;

    /// <summary>
    /// How long typing has to pause before the document is written to.
    ///
    /// Short enough that the page keeps up with the reader, long enough that a
    /// word is one write rather than five.
    /// </summary>
    private static readonly TimeSpan LiveEditPause = TimeSpan.FromMilliseconds(180);

    private readonly DispatcherTimer _liveEdit = new() { Interval = LiveEditPause };

    /// <summary>
    /// Whether the timer's handler is attached. Attached once, on the first
    /// editor ever opened, rather than per editor: pairing a subscribe with an
    /// unsubscribe across an async commit is the kind of bookkeeping that
    /// silently leaves two handlers on the timer and writes twice per pause.
    /// </summary>
    private bool _liveEditWired;

    /// <summary>
    /// Set when the core refused the text in the box, cleared as soon as it
    /// changes. It stops the pump from re-sending a rejection on every pass,
    /// and stops Enter from closing an editor whose text never landed.
    /// </summary>
    private bool _liveEditFailed;

    /// <summary>
    /// Queues the editor's current text to be written to the document, after a
    /// short pause in typing.
    /// </summary>
    /// <remarks>
    /// The document keeps up with the keyboard instead of waiting for Enter,
    /// which is affordable because the core amends the queued command rather
    /// than appending one per keystroke — the whole typing session stays a
    /// single undo step — and because re-rendering one page costs about ten
    /// milliseconds. The pause exists so a burst of keystrokes is one write,
    /// not one per letter.
    /// </remarks>
    private void ScheduleLiveEdit()
    {
        _liveEdit.Stop();
        _liveEdit.Start();
    }

    private void ContentEditor_TextChanged(object sender, TextChangedEventArgs args) => ScheduleLiveEdit();

    private void LiveEdit_Tick(object? sender, object args)
    {
        _liveEdit.Stop();
        _ = PumpEditorTextAsync();
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
    private async Task PumpEditorTextAsync()
    {
        if (_contentCommit is not null)
        {
            return;
        }

        while (_contentEditor is { } editor && _session is not null && !_liveEditFailed)
        {
            var text = editor.Box.Text;
            if (text == CurrentTextOf(editor))
            {
                return;
            }

            var write = SendEditorTextAsync(editor, text);
            _contentCommit = write;
            try
            {
                await write;
            }
            finally
            {
                _contentCommit = null;
            }
        }
    }

    /// <summary>
    /// Resolves the open editor: makes sure what it says has reached the
    /// document, then closes it. A click on another run, leaving the mode, and
    /// pressing Enter all arrive here.
    /// </summary>
    private async Task CommitContentEditorAsync()
    {
        _liveEdit.Stop();
        if (_contentCommit is { } inFlight)
        {
            await inFlight;
        }

        await PumpEditorTextAsync();

        if (_contentEditor is { } editor && !_liveEditFailed)
        {
            CloseEditor(editor);
        }
    }

    /// <summary>The text the document currently holds for the run being edited.</summary>
    private string CurrentTextOf(ContentEditor editor) =>
        _pendingRunText.TryGetValue((editor.PageIndex, editor.Run.Id), out var pending) ? pending : editor.Run.Text;

    /// <summary>
    /// Sends one editor's text to the core and re-renders the page it is on.
    /// </summary>
    /// <remarks>
    /// Never touches the box. It stays enabled and focused through every
    /// write, because the reader is still typing into it — and a failure is
    /// reported without closing it, so the one failure that matters (a
    /// character the run's font cannot encode) is fixed by typing something
    /// else rather than by finding the run again.
    /// </remarks>
    private async Task SendEditorTextAsync(ContentEditor editor, string text)
    {
        if (_session is null)
        {
            return;
        }

        var sessionId = _session.SessionId;
        var result = await _facade.ReplaceTextRunAsync(sessionId, editor.Run, text);
        if (_session is null || _session.SessionId != sessionId)
        {
            // Another document arrived while the write was in flight; its own
            // reset already cleared this editor.
            return;
        }

        if (!result.IsSuccess)
        {
            // Latched until the text changes again: without this, the pump
            // would re-send the same rejected text on every pass.
            _liveEditFailed = true;
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        _pendingRunText[(editor.PageIndex, editor.Run.Id)] = text;
        _contentEditedPages.Add(editor.PageIndex);
        // The page says something else now, so the characters cached for
        // drag-select no longer describe it.
        InvalidatePageCharacters(editor.PageIndex);
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        InvalidatePageRender(editor.PageIndex);
        AnnotationStatus.Text = "Text updated. Save to keep the change.";
    }
}
