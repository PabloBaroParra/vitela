using Microsoft.UI.Text;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;
using Windows.UI.Text;

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

        if (ContentHitTest.TextRunAt(content.TextRuns, BoundsOf, point.X, point.Y) is not { } run)
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

        OpenEditorOver(pageIndex, run, point.X);
    }

    private void OpenEditorOver(uint pageIndex, ContentTextRun run, double clickX)
    {
        if (pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[(int)pageIndex];
        Brush paper = new SolidColorBrush(Microsoft.UI.Colors.White);
        Brush ink = new SolidColorBrush(Microsoft.UI.Colors.Black);
        // The paper and the text are two elements, not one box with a
        // background. The words being replaced have to be hidden by something
        // shaped exactly like the run — one em tall, no more — while the text
        // itself needs the taller line box its font asks for. One control
        // cannot be both without covering the lines above and below.
        var mask = new Rectangle { Fill = paper, IsHitTestVisible = false };
        var box = new TextBox
        {
            Text = _pendingRunText.TryGetValue((pageIndex, run.Id), out var pending) ? pending : run.Text,
            // No chrome at all: this has to read as the page, not as a field
            // floating over it.
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            MinWidth = 0,
            MinHeight = 0,
            TextWrapping = TextWrapping.NoWrap,
            Background = new SolidColorBrush(Microsoft.UI.Colors.Transparent),
            Foreground = ink,
            CornerRadius = new CornerRadius(0),
            VerticalContentAlignment = VerticalAlignment.Top,
        };
        DressForRun(box, run);
        if (!_liveEditWired)
        {
            _liveEdit.Tick += LiveEdit_Tick;
            _liveEditWired = true;
        }

        MakeEditorLookLikeThePage(box, ink);
        box.KeyDown += ContentEditor_KeyDown;
        box.TextChanged += ContentEditor_TextChanged;

        _contentEditor = new ContentEditor(pageIndex, run, box, mask, box.Text);
        slot.Content.Children.Add(mask);
        slot.Content.Children.Add(box);
        PlaceEditor(slot, pageIndex, _contentEditor);
        box.Focus(FocusState.Programmatic);
        // The caret lands where the reader clicked instead of the whole run
        // being selected: they asked to edit this text, not to replace it, and
        // a run highlighted end to end reads as a field about to be
        // overwritten.
        box.Select(CaretIndexFor(run, box.Text, clickX), 0);
        AnnotationStatus.Text = "Type to edit the text — Escape puts it back.";
    }

    private void ContentEditor_KeyDown(object sender, KeyRoutedEventArgs args)
    {
        // Whatever was refused is being retyped, so let the pump try again.
        _liveEditFailed = false;

        if (args.Key == VirtualKey.Enter)
        {
            args.Handled = true;
            _ = CommitContentEditorAsync();
            return;
        }

        if (args.Key == VirtualKey.Escape)
        {
            args.Handled = true;
            _ = AbandonContentEditorAsync();
        }
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
        _liveEdit.Stop();
        _liveEditFailed = false;
        editor.Box.KeyDown -= ContentEditor_KeyDown;
        editor.Box.TextChanged -= ContentEditor_TextChanged;
        if (editor.PageIndex < _slots.Count)
        {
            var content = _slots[(int)editor.PageIndex].Content;
            content.Children.Remove(editor.Box);
            content.Children.Remove(editor.Mask);
        }

        if (ReferenceEquals(_contentEditor, editor))
        {
            _contentEditor = null;
        }
    }

    /// <summary>
    /// Dresses the editor in the face the page paints this run with.
    /// </summary>
    /// <remarks>
    /// Not a nicety. The editor draws over the page, so a face with different
    /// advance widths puts the reader's text where the document's would never
    /// be — every letter drifting further from the one it replaced. The
    /// document names its font, the core reports that name
    /// (<c>page_font_families</c>), and <see cref="PdfFontMatch"/> decides
    /// which local face answers to it.
    /// </remarks>
    private static void DressForRun(Control box, ContentTextRun run)
    {
        var (families, bold, italic) = PdfFontMatch.ForBaseFont(run.BaseFont);
        box.FontFamily = new FontFamily(families);
        box.FontWeight = bold ? FontWeights.Bold : FontWeights.Normal;
        box.FontStyle = italic ? FontStyle.Italic : FontStyle.Normal;
    }

    /// <summary>
    /// Where the baseline sits inside a run's box, as a fraction of the box
    /// height measured from its top.
    /// </summary>
    /// <remarks>
    /// Not a guess. The parser builds a run's box as exactly one em, from
    /// 0.75 em above the baseline to 0.25 em below it — <c>FALLBACK_ASCENT</c>
    /// and <c>FALLBACK_DESCENT</c> in <c>pdf-edit</c>'s
    /// <c>run_bounding_box</c>. Reading that split the same way here is what
    /// puts the editor's text on the line the page draws on instead of near
    /// it. If the parser ever takes real metrics from the font descriptor,
    /// this has to follow it.
    /// </remarks>
    private const double RunBaselineFromTop = 0.75;

    /// <summary>
    /// The character the caret goes in front of, for a click at
    /// <paramref name="xPt"/> in page space.
    /// </summary>
    /// <remarks>
    /// Proportional, because the exact answer needs the advance width of every
    /// glyph in the document's own font, and the run's box only reports their
    /// sum. Close enough to land in the word that was clicked, which is what
    /// the click was asking for.
    /// </remarks>
    private static int CaretIndexFor(ContentTextRun run, string text, double xPt)
    {
        if (text.Length == 0 || run.Bounds.Width <= 0)
        {
            return 0;
        }

        var fraction = (xPt - run.Bounds.X) / run.Bounds.Width;
        return Math.Clamp((int)Math.Round(fraction * text.Length), 0, text.Length);
    }

    /// <summary>
    /// Strips a <see cref="TextBox"/> of every piece of chrome its default
    /// template paints.
    /// </summary>
    /// <remarks>
    /// The plain properties are not enough. WinUI's template swaps the
    /// background and border to theme brushes in its pointer-over, focused and
    /// disabled visual states, so a box de-chromed only through
    /// <c>Background</c> and <c>BorderThickness</c> grows a border and a grey
    /// fill the moment it takes focus — which is always, since it is opened to
    /// be typed in. Overriding the theme resources on the instance is what
    /// makes the states agree with the properties.
    ///
    /// The fill is the page's paper, not transparent: it is what hides the
    /// words being replaced. A page that is not white will show this, and the
    /// honest fix then is to sample the rendered page rather than to guess
    /// harder here.
    /// </remarks>
    private static void MakeEditorLookLikeThePage(TextBox box, Brush ink)
    {
        var invisible = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        var none = new Thickness(0);
        box.Resources["TextControlBackground"] = invisible;
        box.Resources["TextControlBackgroundPointerOver"] = invisible;
        box.Resources["TextControlBackgroundFocused"] = invisible;
        box.Resources["TextControlBackgroundDisabled"] = invisible;
        box.Resources["TextControlBorderBrush"] = invisible;
        box.Resources["TextControlBorderBrushPointerOver"] = invisible;
        box.Resources["TextControlBorderBrushFocused"] = invisible;
        box.Resources["TextControlBorderBrushDisabled"] = invisible;
        box.Resources["TextControlBorderThemeThickness"] = none;
        box.Resources["TextControlBorderThemeThicknessFocused"] = none;
        box.Resources["TextControlForeground"] = ink;
        box.Resources["TextControlForegroundPointerOver"] = ink;
        box.Resources["TextControlForegroundFocused"] = ink;
        box.Resources["TextControlForegroundDisabled"] = ink;
        box.Resources["TextControlThemePadding"] = none;
    }

    /// <summary>
    /// Puts the editor box exactly over the run it is editing, at the current
    /// zoom, so typing happens where the words are rather than in a panel over
    /// them.
    /// </summary>
    /// <remarks>
    /// The box matches the run's height exactly — that is what makes it cover
    /// the old text — and the font size is read from that height, since a
    /// run's box is drawn around the glyphs it paints. It is allowed to be
    /// wider than the run, because a replacement is usually longer than what
    /// it replaces and a box clipped to the old text would hide what is being
    /// typed; the saved page will overflow to the right in exactly the same
    /// way.
    /// </remarks>
    private void PlaceEditor(PageSlot slot, uint pageIndex, ContentEditor editor)
    {
        if (_session is null || pageIndex >= _session.Pages.Count)
        {
            return;
        }

        var page = _session.Pages[(int)pageIndex];
        var scale = slot.Scale;
        var bounds = BoundsOf(editor.Run);
        // A run's box is one em tall, so its height *is* the size the page
        // draws this text at. No fitting, no factor.
        var em = bounds.Height * scale;
        var left = bounds.X * scale;
        var top = (page.HeightPt - bounds.Y - bounds.Height) * scale;

        var probe = new TextBlock
        {
            Text = editor.Box.Text,
            FontFamily = editor.Box.FontFamily,
            FontWeight = editor.Box.FontWeight,
            FontStyle = editor.Box.FontStyle,
            FontSize = em,
        };
        probe.Measure(new global::Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
        var lineHeight = Math.Max(em, probe.DesiredSize.Height);
        var typedWidth = probe.DesiredSize.Width;

        // The paper covers the run, and once the replacement outgrows it,
        // whatever the replacement now runs over — which is what the saved
        // page covers too, since a longer run overprints its neighbour rather
        // than reflowing away from it.
        editor.Mask.Width = Math.Max(bounds.Width * scale, typedWidth);
        editor.Mask.Height = em;
        Canvas.SetLeft(editor.Mask, left);
        Canvas.SetTop(editor.Mask, top);

        editor.Box.Width = Math.Max(40, typedWidth + em);
        editor.Box.Height = lineHeight;
        editor.Box.FontSize = em;
        Canvas.SetLeft(editor.Box, left);
        // Placed by its baseline, not by its top: the control's line box is
        // taller than the em it draws, and how much taller is the font's
        // business — so it is measured, not assumed.
        Canvas.SetTop(editor.Box, top + (em * RunBaselineFromTop) - probe.BaselineOffset);
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
    private void ForgetPendingContentText()
    {
        _pendingRunText.Clear();
        _pendingRunBounds.Clear();
    }

    /// <summary>The inline editor currently open, and the run it will rewrite.</summary>
    /// <summary>
    /// The editor currently open.
    /// </summary>
    /// <param name="OpenedWith">
    /// What the run said when the box opened — the page's own text, or the
    /// text an earlier unsaved edit gave it. Escape restores exactly this,
    /// which is only knowable at open time: by the time it is pressed the
    /// document has already been written to.
    /// </param>
    private sealed record ContentEditor(uint PageIndex, ContentTextRun Run, TextBox Box, Rectangle Mask, string OpenedWith);
}
