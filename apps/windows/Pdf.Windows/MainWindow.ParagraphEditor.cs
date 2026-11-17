using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;
using Windows.UI.Core;

namespace Pdf.Windows;

/// <summary>
/// The multi-line editor content-edit mode opens over a paragraph block —
/// lines of one font stacked a steady leading apart, which the core reads as
/// one block (PR #568) — the Windows port of the GTK shell's
/// <c>content_edit::paragraph_editor</c>.
/// </summary>
/// <remarks>
/// A one-line box cannot hold a paragraph: its line breaks are part of what
/// the reader retypes, and the core wraps each typed line to the paragraph's
/// width. So a paragraph gets a wrapping box the paragraph's width, where
/// <b>Enter</b> is a line break, <b>Ctrl+Enter</b> or a click elsewhere
/// commits, and <b>Escape</b> cancels (<see cref="ParagraphEditing"/>).
///
/// Unlike the one-line editor, the paragraph is written only when the box
/// commits — as on Linux. Everything else about the box (opening, closing,
/// refusals, the restyle and delete paths) is the one-line editor's, in
/// <c>MainWindow.ContentEditor.cs</c> and <c>MainWindow.ContentEditWriter.cs</c>.
/// </remarks>
public sealed partial class MainWindow
{
    /// <summary>The narrowest the box is drawn, in pixels, so a paragraph at a tiny zoom still shows what is typed.</summary>
    private const double ParagraphEditorMinWidth = 80;

    /// <summary>
    /// The paragraph box's keys, read ahead of its own: the box would take
    /// Ctrl+Enter as a line break too.
    /// </summary>
    private void ParagraphEditor_PreviewKeyDown(object sender, KeyRoutedEventArgs args)
    {
        var enter = args.Key == VirtualKey.Enter;
        var escape = args.Key == VirtualKey.Escape;
        // A read-only box still sees Enter and Escape: while a restyle runs in
        // it, a commit or undo here would land on the restyle, not the typing.
        if (_restyleInEditor && (enter || escape))
        {
            args.Handled = true;
            return;
        }

        // Whatever was refused is being retyped, so let the pump try again.
        _pump.Retry();
        var control = InputKeyboardSource.GetKeyStateForCurrentThread(VirtualKey.Control).HasFlag(CoreVirtualKeyStates.Down);
        switch (ParagraphEditing.KeyAction(enter, escape, control))
        {
            case ParagraphKeyAction.Commit:
                args.Handled = true;
                _ = CommitContentEditorAsync();
                break;
            case ParagraphKeyAction.Cancel:
                args.Handled = true;
                _ = AbandonContentEditorAsync();
                break;
        }
    }

    /// <summary>
    /// Puts the paragraph box over <paramref name="paragraph"/> at the current
    /// zoom: the paragraph's width, so it wraps about where the page will, its
    /// first baseline on the paragraph's, and tall enough for every line typed.
    /// </summary>
    /// <remarks>
    /// The font size is the first line's, read from its run's height — a run's
    /// box is one em tall. The box's own line spacing is its font's, not the
    /// paragraph's leading, so lines past the first sit near the page's rather
    /// than on them; the paper under the box hides the page's text either way.
    /// </remarks>
    private void PlaceParagraphEditor(PageSlot slot, uint pageIndex, ContentEditor editor, ContentTextBlock paragraph)
    {
        var scale = slot.Scale;
        var bounds = paragraph.Bounds;
        var em = paragraph.First.Bounds.Height * scale;
        var width = Math.Max(ParagraphEditorMinWidth, bounds.Width * scale);

        var probe = new TextBlock
        {
            // A trailing line break still takes a line in the box.
            Text = ParagraphEditing.FromTextBox(editor.Box.Text) + " ",
            FontFamily = editor.Box.FontFamily,
            FontWeight = editor.Box.FontWeight,
            FontStyle = editor.Box.FontStyle,
            FontSize = em,
            TextWrapping = TextWrapping.Wrap,
        };
        probe.Measure(new global::Windows.Foundation.Size(width, double.PositiveInfinity));
        var height = Math.Ceiling(Math.Max(em, probe.DesiredSize.Height));

        editor.Box.FontSize = em;
        editor.Box.Width = width;
        editor.Box.Height = height;
        // Placed by its first baseline, as the one-line box is.
        var offsetY = (em * RunBaselineFromTop) - probe.BaselineOffset;
        PlaceUpright(editor.Box, slot, (int)pageIndex, bounds, offsetY: offsetY);

        // The paper covers the paragraph, and whatever the typed lines now run
        // over below it.
        editor.Mask.Width = width;
        editor.Mask.Height = Math.Max(bounds.Height * scale, height + offsetY);
        PlaceUpright(editor.Mask, slot, (int)pageIndex, bounds);
    }

    /// <summary>
    /// After a write, the open paragraph editor retypes the paragraph as the
    /// page holds it now: a retype built from the runs it opened over is
    /// refused once a write grew a line. Only a flush mid-edit — a restyle —
    /// writes before the box closes.
    /// </summary>
    private void RefindParagraphAfterWrite(ContentEditor editor, string written)
    {
        if (!editor.IsParagraph || editor.Block is not { } opened) return;
        if (_pageContent.TryGetValue(editor.PageIndex, out var state) && state.Content is { } content
            && ParagraphEditing.Refind(content.TextBlocks, opened, written) is { } now)
        {
            editor.Block = now;
        }
    }

    /// <summary>
    /// Where the caret goes in a paragraph's text for a click at
    /// <paramref name="xPt"/> on <paramref name="run"/>: in that run's line,
    /// in front of the character the click was over.
    /// </summary>
    private static int ParagraphCaretIndex(ContentTextBlock paragraph, ContentTextRun run, double xPt)
    {
        var start = paragraph.Text.IndexOf(run.Text, StringComparison.Ordinal);
        return start < 0 ? paragraph.Text.Length : start + CaretIndexFor(run, run.Text, xPt);
    }
}
