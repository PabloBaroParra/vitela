using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
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
        Brush paper = new SolidColorBrush(Microsoft.UI.Colors.White);
        Brush ink = new SolidColorBrush(Microsoft.UI.Colors.Black);
        var box = new TextBox
        {
            Text = _pendingRunText.TryGetValue((pageIndex, run.Id), out var pending) ? pending : run.Text,
            // No chrome at all: the box has to read as the page, not as a
            // field floating over it. It covers the run's own box, so the
            // words underneath are hidden while the new ones are typed —
            // without that the reader would see both at once.
            BorderThickness = new Thickness(0),
            Padding = new Thickness(0),
            MinWidth = 0,
            MinHeight = 0,
            TextWrapping = TextWrapping.NoWrap,
            Background = paper,
            Foreground = ink,
            FontFamily = new FontFamily(EditorFontFamily),
            CornerRadius = new CornerRadius(0),
        };
        if (!_liveEditWired)
        {
            _liveEdit.Tick += LiveEdit_Tick;
            _liveEditWired = true;
        }

        MakeEditorLookLikeThePage(box, paper, ink);
        box.KeyDown += ContentEditor_KeyDown;
        box.TextChanged += ContentEditor_TextChanged;

        _contentEditor = new ContentEditor(pageIndex, run, box);
        slot.Content.Children.Add(box);
        PlaceEditor(slot, pageIndex, _contentEditor);
        box.Focus(FocusState.Programmatic);
        box.SelectAll();
        AnnotationStatus.Text = "Retype the text and press Enter — Escape cancels.";
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
            CancelContentEditor();
            AnnotationStatus.Text = "Edit cancelled.";
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
            _slots[(int)editor.PageIndex].Content.Children.Remove(editor.Box);
        }

        if (ReferenceEquals(_contentEditor, editor))
        {
            _contentEditor = null;
        }
    }

    /// <summary>
    /// The face the editor types in until the page is re-rendered.
    /// </summary>
    /// <remarks>
    /// A stand-in, and only for the seconds between the first keystroke and
    /// the commit: once the edit lands, PDFium redraws the page in the
    /// document's own font. A run reports its <em>kind</em>, not its family,
    /// so matching the document properly would mean carrying the base font
    /// across the FFI — a core change for a few seconds of fidelity, and a
    /// candidate for later if this reads wrong in practice.
    /// </remarks>
    private const string EditorFontFamily = "Segoe UI";

    /// <summary>
    /// The size at which the stand-in face fills the same box the document's
    /// own text does.
    /// </summary>
    /// <remarks>
    /// Height alone is not enough. A run's box spans ascender to descender, so
    /// a size set from it draws taller than the glyphs it covers, and two
    /// faces at the same size still run to different widths — the reader sees
    /// the replacement land wider or narrower than the words it replaced. So
    /// the height gives a starting size and the run's own width corrects it:
    /// whatever size makes the *original* text measure the width the page
    /// actually gave it is the size that matches the page's density.
    ///
    /// Clamped, because the correction is only as good as the measurement: a
    /// run of one character, or one the layout engine refuses to measure,
    /// must not be allowed to produce absurd text.
    /// </remarks>
    private static double FittedFontSize(ContentTextRun run, double heightPx, double widthPx)
    {
        var fromHeight = Math.Max(6, heightPx * 0.82);
        if (run.Text.Length == 0 || widthPx <= 0)
        {
            return fromHeight;
        }

        var probe = new TextBlock
        {
            Text = run.Text,
            FontFamily = new FontFamily(EditorFontFamily),
            FontSize = fromHeight,
        };
        probe.Measure(new global::Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
        var measured = probe.DesiredSize.Width;
        if (measured <= 0)
        {
            return fromHeight;
        }

        return Math.Clamp(fromHeight * (widthPx / measured), fromHeight * 0.6, fromHeight * 1.4);
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
    private static void MakeEditorLookLikeThePage(TextBox box, Brush paper, Brush ink)
    {
        var invisible = new SolidColorBrush(Microsoft.UI.Colors.Transparent);
        var none = new Thickness(0);
        box.Resources["TextControlBackground"] = paper;
        box.Resources["TextControlBackgroundPointerOver"] = paper;
        box.Resources["TextControlBackgroundFocused"] = paper;
        box.Resources["TextControlBackgroundDisabled"] = paper;
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
        var bounds = editor.Run.Bounds;
        var height = bounds.Height * scale;
        editor.Box.Width = Math.Max(40, bounds.Width * scale * 1.6);
        editor.Box.Height = height;
        editor.Box.FontSize = FittedFontSize(editor.Run, height, bounds.Width * scale);
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
