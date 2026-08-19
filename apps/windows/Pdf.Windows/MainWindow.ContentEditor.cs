using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// The inline editor content-edit mode opens over a text run: where it goes,
/// what it shows, and what happens to the text in it.
///
/// Split from <c>MainWindow.ContentEdit.cs</c>, which owns the mode itself —
/// arming it, routing a click, and holding the parsed page content. This owns
/// one box's life: open over a run, place it at the current zoom, commit or
/// cancel. The seam matches the GTK shell's, where <c>content_edit::editor</c>
/// is its own module beside <c>content_edit::mod</c>.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>
    /// Text committed against a run but not yet re-read from the file, keyed
    /// by page and run id.
    ///
    /// The parse this shell holds describes the document as opened, and a
    /// pending edit never changes it — that is deliberate, since the ids in it
    /// are what the core matches an edit against. But the reader sees the new
    /// words on the page, so re-opening the editor on that run has to show
    /// them too, not the text they replaced.
    /// </summary>
    private readonly Dictionary<(uint Page, ulong Run), string> _pendingRunText = [];

    private ContentEditor? _contentEditor;

    /// <summary>The commit in flight, so a second one waits for it instead of racing it.</summary>
    private Task? _contentCommit;

    /// <summary>
    /// Resolves whatever editor is open, then opens one over the run under
    /// <paramref name="point"/>.
    /// </summary>
    private async Task OpenContentEditorAsync(uint pageIndex, AnnotationPoint point)
    {
        await CommitContentEditorAsync();
        if (_session is null || !_contentEditMode)
        {
            return;
        }

        var content = await EnsurePageContentAsync(pageIndex);
        if (content is null || _session is null || !_contentEditMode)
        {
            return;
        }

        if (ContentHitTest.TextRunAt(content.TextRuns, point.X, point.Y) is not { } run)
        {
            AnnotationStatus.Text = "No editable text there — click a word to retype it.";
            return;
        }

        if (!run.IsEditable)
        {
            // Refused before an editor opens rather than after the reader has
            // typed: the core rejects every replacement against a composite
            // font, so a box here could only ever fail.
            AnnotationStatus.Text = "This text uses a font this version cannot retype (composite/CID).";
            return;
        }

        OpenEditorOver(pageIndex, run);
    }

    private void OpenEditorOver(uint pageIndex, ContentTextRun run)
    {
        if (pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[(int)pageIndex];
        var box = new TextBox
        {
            Text = _pendingRunText.TryGetValue((pageIndex, run.Id), out var pending) ? pending : run.Text,
            BorderThickness = new Thickness(1),
            Padding = new Thickness(0),
            MinWidth = 0,
            MinHeight = 0,
            TextWrapping = TextWrapping.NoWrap,
        };
        box.KeyDown += ContentEditor_KeyDown;

        _contentEditor = new ContentEditor(pageIndex, run, box);
        slot.Content.Children.Add(box);
        PlaceEditor(slot, pageIndex, _contentEditor);
        box.Focus(FocusState.Programmatic);
        box.SelectAll();
        AnnotationStatus.Text = "Retype the text and press Enter — Escape cancels.";
    }

    private void ContentEditor_KeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.Key == VirtualKey.Enter)
        {
            args.Handled = true;
            _ = CommitContentEditorAsync();
            return;
        }

        if (args.Key == VirtualKey.Escape)
        {
            args.Handled = true;
            CancelContentEditor();
            AnnotationStatus.Text = "Edit cancelled.";
        }
    }

    /// <summary>
    /// Resolves the open editor: records its text if it changed, then closes
    /// it. Concurrent callers await the same commit rather than starting a
    /// second one — a click on another run resolves the box in progress, and
    /// pressing Enter and clicking away are the same gesture arriving twice.
    /// </summary>
    private async Task CommitContentEditorAsync()
    {
        if (_contentCommit is { } inFlight)
        {
            await inFlight;
            return;
        }

        if (_contentEditor is not { } editor || _session is null)
        {
            return;
        }

        var text = editor.Box.Text;
        var current = _pendingRunText.TryGetValue((editor.PageIndex, editor.Run.Id), out var pending) ? pending : editor.Run.Text;
        if (text == current)
        {
            CloseEditor(editor);
            return;
        }

        // Assigned before the first continuation runs — this all happens on
        // the UI thread — so a reentrant caller sees the task, not a null.
        var commit = RecordEditorTextAsync(editor, text);
        _contentCommit = commit;
        try
        {
            await commit;
        }
        finally
        {
            _contentCommit = null;
        }
    }

    /// <summary>
    /// Sends one editor's text to the core, then re-renders the pages so the
    /// reader sees the words the PDF now paints.
    /// </summary>
    /// <remarks>
    /// A failure keeps the editor open with the text still in it. The one that
    /// matters is a character the run's font cannot encode: the reader can fix
    /// that by typing something else, and closing the box would make them find
    /// the run again first.
    /// </remarks>
    private async Task RecordEditorTextAsync(ContentEditor editor, string text)
    {
        if (_session is null)
        {
            return;
        }

        var sessionId = _session.SessionId;
        editor.Box.IsEnabled = false;
        var result = await _facade.ReplaceTextRunAsync(sessionId, editor.Run, text);
        if (_session is null || _session.SessionId != sessionId)
        {
            // Another document arrived while the edit was in flight; its own
            // reset already cleared this editor.
            return;
        }

        if (!result.IsSuccess)
        {
            editor.Box.IsEnabled = true;
            editor.Box.Focus(FocusState.Programmatic);
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }

        _pendingRunText[(editor.PageIndex, editor.Run.Id)] = text;
        CloseEditor(editor);
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        InvalidateRenderedPages();
        AnnotationStatus.Text = "Text updated. Save to keep the change.";
    }

    /// <summary>Closes the open editor without recording anything.</summary>
    private void CancelContentEditor()
    {
        if (_contentEditor is { } editor)
        {
            CloseEditor(editor);
        }
    }

    /// <summary>
    /// Takes one editor's box off the page.
    /// </summary>
    /// <remarks>
    /// Named for the editor rather than reading <see cref="_contentEditor"/>
    /// because a commit finishing late must remove <em>its own</em> box, not
    /// whichever one happens to be open by then — and must leave the current
    /// editor alone if a newer one has already replaced it.
    /// </remarks>
    private void CloseEditor(ContentEditor editor)
    {
        editor.Box.KeyDown -= ContentEditor_KeyDown;
        if (editor.PageIndex < _slots.Count)
        {
            _slots[(int)editor.PageIndex].Content.Children.Remove(editor.Box);
        }

        if (ReferenceEquals(_contentEditor, editor))
        {
            _contentEditor = null;
        }
    }

    /// <summary>
    /// Puts the editor box exactly over the run it is editing, at the current
    /// zoom. The font size follows the run's own height, so what the reader
    /// types is about the size of what the page will paint — the page itself
    /// is the authority, once the edit lands.
    /// </summary>
    private void PlaceEditor(PageSlot slot, uint pageIndex, ContentEditor editor)
    {
        if (_session is null || pageIndex >= _session.Pages.Count)
        {
            return;
        }

        var page = _session.Pages[(int)pageIndex];
        var scale = slot.Scale;
        var bounds = editor.Run.Bounds;
        // Wider than the run: the replacement is usually longer than what it
        // replaces, and a box clipped to the old text hides what is being
        // typed.
        editor.Box.Width = Math.Max(60, bounds.Width * scale * 1.5);
        editor.Box.Height = Math.Max(18, bounds.Height * scale * 1.6);
        editor.Box.FontSize = Math.Max(8, bounds.Height * scale);
        Canvas.SetLeft(editor.Box, bounds.X * scale);
        Canvas.SetTop(editor.Box, (page.HeightPt - bounds.Y - bounds.Height) * scale);
    }

    /// <summary>
    /// Forgets which runs carry an unsaved retype, after an undo or redo.
    ///
    /// Nothing here knows which command a history step moved, so the honest
    /// answer is none of them: the editor then prefills from the parsed page
    /// rather than from text that may no longer be queued. The parsed content
    /// itself stays — its ids are keyed to the opened bytes, which a history
    /// step does not touch.
    /// </summary>
    private void ForgetPendingContentText() => _pendingRunText.Clear();

    /// <summary>The inline editor currently open, and the run it will rewrite.</summary>
    private sealed record ContentEditor(uint PageIndex, ContentTextRun Run, TextBox Box);
}
