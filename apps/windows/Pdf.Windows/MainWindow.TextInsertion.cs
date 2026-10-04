using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// The blank text box "Insert text" opens where the page was clicked, and the
/// one insertion it records.
/// </summary>
/// <remarks>
/// Deliberately not the retype editor (<see cref="ContentEditPump{TBox}"/>).
/// That one rewrites an existing run on every pause in typing, amending its
/// own queued command; an inserted run cannot be amended through the FFI yet,
/// so this box writes exactly once — on Enter, on losing focus, or when the
/// next page click resolves it — and Escape records nothing. The GTK shell's
/// <c>editor::open_insert_editor</c> uses the same fixed default box: 14 pt
/// Helvetica, anchored by its bottom-left corner at the click.
/// </remarks>
public sealed partial class MainWindow
{
    private const double InsertTextSizePt = 14;
    private const double InsertTextWidthPt = 150;

    private sealed record InsertEditor(string SessionId, TextInsertionTarget Target, AnnotationPoint Anchor, TextBox Box);

    private InsertEditor? _insertEditor;
    private Task _insertCommit = Task.CompletedTask;

    private async Task OpenInsertEditorAsync(uint pageIndex, AnnotationPoint point)
    {
        if (_session is null) return;
        var sessionId = _session.SessionId;
        var generation = _contentModeGeneration;
        // A write still in flight from the previous box moves the revision a
        // fresh target is stamped with.
        await CommitInsertEditorAsync();
        if (_session?.SessionId != sessionId || _contentModeGeneration != generation
            || _contentInsertKind != ContentInsertKind.Text || pageIndex >= _slots.Count) return;

        var prepared = await _facade.PrepareTextInsertionAsync(sessionId, pageIndex);
        if (_session?.SessionId != sessionId || _contentModeGeneration != generation
            || _contentInsertKind != ContentInsertKind.Text || pageIndex >= _slots.Count) return;
        if (!prepared.IsSuccess)
        {
            AnnotationStatus.Text = prepared.Error!.Message;
            return;
        }

        Brush ink = new SolidColorBrush(Microsoft.UI.Colors.Black);
        var box = new TextBox
        {
            Padding = new Thickness(0),
            MinWidth = 0,
            MinHeight = 0,
            TextWrapping = TextWrapping.NoWrap,
            Foreground = ink,
            CornerRadius = new CornerRadius(0),
            VerticalContentAlignment = VerticalAlignment.Top,
        };
        var (families, _, _) = PdfFontMatch.ForBaseFont("Helvetica");
        box.FontFamily = new FontFamily(families);
        // Unlike a retype, there are no words underneath to read as the box:
        // without an outline the reader cannot see where the text will land.
        MakeEditorLookLikeThePage(box, ink, outline: new SolidColorBrush(HandleBrush.Color));

        var editor = new InsertEditor(sessionId, prepared.Value!, point, box);
        box.KeyDown += InsertEditor_KeyDown;
        box.TextChanged += InsertEditor_TextChanged;
        box.LostFocus += InsertEditor_LostFocus;
        _insertEditor = editor;
        _slots[(int)pageIndex].Content.Children.Add(box);
        PlaceInsertEditor(editor);
        box.Focus(FocusState.Programmatic);
        AnnotationStatus.Text = "Type the new text — Enter inserts it, Escape cancels.";
    }

    private void InsertEditor_KeyDown(object sender, KeyRoutedEventArgs args)
    {
        if (args.Key == VirtualKey.Enter)
        {
            args.Handled = true;
            _ = CommitInsertEditorAsync();
        }
        else if (args.Key == VirtualKey.Escape)
        {
            args.Handled = true;
            CancelInsertEditor();
            AnnotationStatus.Text = "Insertion cancelled.";
        }
    }

    private void InsertEditor_TextChanged(object sender, TextChangedEventArgs args)
    {
        if (_insertEditor is { } editor && ReferenceEquals(editor.Box, sender)) PlaceInsertEditor(editor);
    }

    private void InsertEditor_LostFocus(object sender, RoutedEventArgs args)
    {
        if (_insertEditor is { } editor && ReferenceEquals(editor.Box, sender)) _ = CommitInsertEditorAsync();
    }

    /// <summary>
    /// Records the open box's text as a new run, or finishes the write already
    /// under way. Idempotent: the box is taken off the page before anything is
    /// awaited, so focus loss, Enter and the next click cannot record it twice.
    /// </summary>
    private Task CommitInsertEditorAsync()
    {
        if (_insertEditor is not { } editor) return _insertCommit;
        TakeInsertEditorOffThePage(editor);
        _insertCommit = WriteInsertionAsync(editor, _insertCommit);
        return _insertCommit;
    }

    private async Task WriteInsertionAsync(InsertEditor editor, Task previous)
    {
        await previous;
        var text = editor.Box.Text;
        // An empty box is a click that changed its mind, not a refusal.
        if (string.IsNullOrWhiteSpace(text) || _session?.SessionId != editor.SessionId) return;
        var pageIndex = editor.Target.PageIndex;
        var result = await _facade.InsertTextRunAsync(editor.SessionId, editor.Target, text,
            editor.Anchor.X, editor.Anchor.Y, InsertTextSizePt);
        if (_session?.SessionId != editor.SessionId) return;
        // A recorded edit remains undoable even if its preview refresh failed.
        _contentEditedPages.Add(pageIndex);
        _pageContent.Remove(pageIndex);
        if (pageIndex < _slots.Count) ClearContentOutlines(_slots[(int)pageIndex]);
        InvalidatePageCharacters(pageIndex);
        InvalidatePageRender(pageIndex);
        RedrawAnnotations();
        if (!result.IsSuccess)
        {
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = result.Error!.Message;
            return;
        }
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        AnnotationStatus.Text = "Text inserted. Save to keep the change.";
    }

    /// <summary>Drops the open box without recording anything.</summary>
    private void CancelInsertEditor()
    {
        if (_insertEditor is { } editor) TakeInsertEditorOffThePage(editor);
    }

    private void TakeInsertEditorOffThePage(InsertEditor editor)
    {
        if (ReferenceEquals(_insertEditor, editor)) _insertEditor = null;
        editor.Box.KeyDown -= InsertEditor_KeyDown;
        editor.Box.TextChanged -= InsertEditor_TextChanged;
        editor.Box.LostFocus -= InsertEditor_LostFocus;
        var page = editor.Target.PageIndex;
        if (page < _slots.Count) _slots[(int)page].Content.Children.Remove(editor.Box);
    }

    /// <summary>
    /// Puts the box where the run will be drawn, at the current zoom: its
    /// baseline a quarter em above the click, which is where
    /// <c>pdf_edit::insert_text_run</c> puts the text of a box anchored there.
    /// </summary>
    private void PlaceInsertEditor(InsertEditor editor)
    {
        var pageIndex = editor.Target.PageIndex;
        if (_session is null || pageIndex >= _slots.Count || pageIndex >= _session.Pages.Count) return;
        var slot = _slots[(int)pageIndex];
        var em = InsertTextSizePt * slot.Scale;
        var probe = new TextBlock { Text = editor.Box.Text, FontFamily = editor.Box.FontFamily, FontSize = em };
        probe.Measure(new global::Windows.Foundation.Size(double.PositiveInfinity, double.PositiveInfinity));
        editor.Box.FontSize = em;
        editor.Box.Width = Math.Max(InsertTextWidthPt * slot.Scale, probe.DesiredSize.Width + em);
        editor.Box.Height = Math.Max(em, probe.DesiredSize.Height) + 2;
        var bounds = new AnnotationRect(editor.Anchor.X, editor.Anchor.Y, InsertTextWidthPt, InsertTextSizePt);
        PlaceUpright(editor.Box, slot, (int)pageIndex, bounds, offsetY: (em * RunBaselineFromTop) - probe.BaselineOffset - 1);
    }
}
