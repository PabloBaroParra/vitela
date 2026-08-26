using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

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
/// The rules about <em>ordering</em> — which write must land before an editor
/// may close, what a history step owes an open box — are not here. They live
/// in <see cref="ContentEditPump{TBox}"/> under <c>Viewer/</c>, for the same
/// reason <see cref="ContentHitTest"/> and <see cref="PdfFontMatch"/> do:
/// every one of them is a rule about a race on the dispatcher thread, and a
/// rule that can only be exercised by opening a window is a rule nobody
/// exercises. What is left in this file is the WinUI end of the pump's three
/// ports — the pause timer, the box, and the write itself.
///
/// Split from <c>MainWindow.ContentEditor.cs</c>, which owns the box's
/// appearance: this owns what a write does to the shell around it. The same
/// seam the GTK shell draws between <c>content_edit::editor</c> and
/// <c>content_edit::command</c>.
/// </summary>
public sealed partial class MainWindow
{
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

    private void LiveEdit_Tick(object? sender, object args)
    {
        _liveEdit.Stop();
        _ = _pump.PumpAsync();
    }

    private void ContentEditor_TextChanged(object sender, TextChangedEventArgs args)
    {
        // Re-laid out on every keystroke, not only on the write: the paper has
        // to keep covering what the text now spans, or a replacement longer
        // than the run it replaces would be typed over the words underneath.
        if (_pump.Box is { } editor && editor.PageIndex < _slots.Count)
        {
            PlaceEditor(_slots[(int)editor.PageIndex], editor.PageIndex, editor);
        }

        _pump.Schedule();
    }

    /// <summary>
    /// Resolves the open editor: makes sure what it says has reached the
    /// document, then closes it. A click on another run, leaving the mode, and
    /// pressing Enter all arrive here.
    /// </summary>
    private Task CommitContentEditorAsync() => _pump.CommitAsync();

    /// <summary>
    /// Puts the run back the way the editor found it, then closes the box.
    /// </summary>
    /// <remarks>
    /// The pump decides which way back applies and does the writing; what is
    /// left here is the half that needs the shell — running the undo through
    /// the facade, and saying so.
    /// </remarks>
    private async Task AbandonContentEditorAsync()
    {
        var outcome = await _pump.AbandonAsync();
        if (outcome == ContentEditAbandon.Undo)
        {
            await ApplyHistoryAsync(undo: true);
        }

        AnnotationStatus.Text = "Edit cancelled.";
    }

    /// <summary>
    /// Settles the inline editor before the edit log itself moves — see
    /// <see cref="ContentEditPump{TBox}.SettleForHistoryAsync"/> for why a
    /// history step cannot leave a box open over the run it is about to move.
    /// </summary>
    private Task SettleContentEditorForHistoryAsync() => _pump.SettleForHistoryAsync();

    /// <summary>
    /// Forgets which runs carry an unsaved retype, after an undo or redo — the
    /// pump's own record, and the boxes this shell measured for them.
    /// </summary>
    private void ForgetPendingContentText()
    {
        _pump.Forget();
        _pendingRunBounds.Clear();
    }

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
    private async Task<ContentWriteOutcome> SendEditorTextAsync(ContentEditor editor, string text)
    {
        if (_session is null)
        {
            return ContentWriteOutcome.Abandoned;
        }

        var sessionId = _session.SessionId;
        var result = editor.Run.RequiresFontSubstitution
            ? await _facade.ReplaceTextRunWithInsertedFontAsync(sessionId, editor.Run, text)
            : await _facade.ReplaceTextRunAsync(sessionId, editor.Run, text);
        if (_session is null || _session.SessionId != sessionId)
        {
            // Another document arrived while the write was in flight; its own
            // reset already cleared this editor.
            return ContentWriteOutcome.Abandoned;
        }

        if (!result.IsSuccess)
        {
            AnnotationStatus.Text = result.Error!.Message;
            return ContentWriteOutcome.Refused;
        }

        RecordPendingBounds(editor.Run, text);
        _contentEditedPages.Add(editor.PageIndex);
        // The page says something else now, so the characters cached for
        // drag-select no longer describe it.
        InvalidatePageCharacters(editor.PageIndex);
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        InvalidatePageRender(editor.PageIndex);
        AnnotationStatus.Text = "Text updated. Save to keep the change.";
        return ContentWriteOutcome.Written;
    }

    /// <summary>The pump's write port, wired to the facade.</summary>
    private sealed class FacadeContentWriter(MainWindow owner) : IContentEditWriter<ContentEditor>
    {
        public Task<ContentWriteOutcome> WriteAsync(ContentEditor box, string text) =>
            owner.SendEditorTextAsync(box, text);
    }

    /// <summary>The pump's pause port, wired to the shell's dispatcher timer.</summary>
    private sealed class DispatcherEditPause(DispatcherTimer timer) : IEditPause
    {
        public void Stop() => timer.Stop();

        public void Restart()
        {
            timer.Stop();
            timer.Start();
        }
    }
}
