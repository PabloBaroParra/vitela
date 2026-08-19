using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using Windows.System;

namespace Pdf.Windows;

/// <summary>
/// Content-edit mode: click a text run the page itself paints and retype it
/// in place, keeping its font, size and position.
///
/// This is not the annotation toolbar. An annotation is drawn *over* the page
/// and this shell owns its pixels, which is why one can be dragged around
/// before anything is saved. A text run *is* the page: only the PDF renderer
/// can draw the new words, so every committed edit ends in a preview refresh
/// (<c>PdfDocumentFacade.ReplaceTextRunAsync</c>) and a re-render of the
/// pages on screen. Showing "changes pending" over a bitmap of the old text
/// is the one outcome this mode must never produce.
///
/// The hit-test, the refusal rules and the amend-on-retype behaviour all live
/// in the core; the GTK shell reaches them by linking <c>pdf-edit</c>
/// directly, this one through the facade. What lives here is presentation:
/// which run a click lands on, where the editor box goes, and what the reader
/// is told.
/// </summary>
public sealed partial class MainWindow
{
    /// <summary>
    /// Every page this session has retyped something on.
    ///
    /// Grows only — an undo does not remove a page from it, because the page
    /// still has to be re-rendered to show the text coming back. Cleared with
    /// the document, like everything else keyed to the bytes it was parsed
    /// from.
    /// </summary>
    private readonly HashSet<uint> _contentEditedPages = [];

    private bool _contentEditMode;

    private void ContentEditButton_Click(object sender, RoutedEventArgs e) =>
        SetContentEditMode(ContentEditButton.IsChecked == true);

    /// <summary>
    /// Turns the mode on or off.
    ///
    /// Mutually exclusive with an armed annotation tool in both directions: one
    /// mode claims a page click at a time. Arming a tool calls in here to turn
    /// this off (see <c>MainWindow.Annotations.cs</c>), and turning this on
    /// disarms whatever was armed.
    /// </summary>
    private async void SetContentEditMode(bool active)
    {
        if (_contentEditMode == active)
        {
            ContentEditButton.IsChecked = active;
            return;
        }

        _contentEditMode = active;
        ContentEditButton.IsChecked = active;

        if (!active)
        {
            // Leaving the mode resolves the edit in progress rather than
            // dropping it — the same thing clicking another run does.
            await CommitContentEditorAsync();
            ClearContentEditVisuals();
            AnnotationStatus.Text = "Content editing off.";
            return;
        }

        _armedAnnotation = null;
        _selectedAnnotationId = null;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        // A selection made before the mode was armed means nothing inside it —
        // content editing never selects text — and leaving it painted would
        // read as state the next click is about to act on.
        _textSelection = null;
        _textDragActive = false;
        RedrawSelection();
        AnnotationStatus.Text = "Content editing armed — click a text run to retype it.";
        UpdateContentEditOverlays(VisiblePageWindow());
    }

    /// <summary>Whether the press was claimed as a content-edit gesture.</summary>
    private bool BeginContentEditPointer(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (!_contentEditMode || _session is null)
        {
            return false;
        }

        // Armed, the mode claims every press on a page: a click that lands on
        // no run still resolves the editor already open, and still says why
        // nothing happened. Falling through to annotations or a drag-select
        // would make an armed mode mean different things in different places
        // on the same page.
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        _ = OpenContentEditorAsync((uint)pageIndex, point);
        return true;
    }

    /// <summary>
    /// Throws away one page's bitmap and tiles so the next viewport pass asks
    /// the core for it again.
    ///
    /// One page, not the document: a content edit rewrites the stream of the
    /// page it targeted and leaves every other page byte-identical, so
    /// dropping them all would re-render the whole visible window to redraw a
    /// single word. Only the *bitmap* is dropped, never the
    /// <see cref="Image"/> source — the page keeps showing the old render,
    /// scaled as usual, until the new one lands, instead of flashing white.
    /// </summary>
    private void InvalidatePageRender(uint pageIndex)
    {
        if (pageIndex >= _slots.Count)
        {
            return;
        }

        var slot = _slots[(int)pageIndex];
        slot.Render.DropBitmap();
        slot.Tiles.Clear();
        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// Re-renders every page this session has retyped something on — what an
    /// undo or a redo needs, since neither says which page it moved.
    /// </summary>
    private void InvalidateContentEditedPages()
    {
        foreach (var pageIndex in _contentEditedPages)
        {
            if (pageIndex < _slots.Count)
            {
                _slots[(int)pageIndex].Render.DropBitmap();
                _slots[(int)pageIndex].Tiles.Clear();
            }
        }

        UpdateViewport(intermediate: false);
    }

    /// <summary>
    /// Keeps the run outlines and the open editor aligned with the pages on
    /// screen: loads content for pages that came into view, and re-places what
    /// is already drawn at the current scale.
    /// </summary>
    private void UpdateContentEditOverlays(PageWindow visible)
    {
        if (!_contentEditMode)
        {
            return;
        }

        for (var index = visible.First; index <= visible.Last && index < _slots.Count; index++)
        {
            _ = EnsurePageContentAsync((uint)index);
        }

        // Outlines exist only for the pages on screen. One rectangle per run
        // is a WinUI element per run, and a page of dense text has hundreds —
        // keeping them for every page the reader has scrolled through would
        // grow the visual tree for pages nobody can see. The parsed content
        // stays cached either way, so coming back is free.
        for (var index = 0; index < _slots.Count; index++)
        {
            if (visible.Contains(index))
            {
                DrawContentOutlines((uint)index);
            }
            else
            {
                ClearContentOutlines(_slots[index]);
            }
        }

        if (_contentEditor is { } editor && editor.PageIndex < _slots.Count)
        {
            PlaceEditor(_slots[(int)editor.PageIndex], editor.PageIndex, editor);
        }
    }

    /// <summary>
    /// Outlines every text run on one page: solid for a run that can be
    /// retyped, dashed and dimmer for one that cannot.
    /// </summary>
    /// <remarks>
    /// The two are told apart on screen because the alternative is a click
    /// that silently does nothing on runs a reader has no way to recognise —
    /// a composite-font run looks exactly like any other text.
    /// </remarks>
    private void DrawContentOutlines(uint pageIndex)
    {
        if (pageIndex >= _slots.Count || _session is null || pageIndex >= _session.Pages.Count)
        {
            return;
        }

        var slot = _slots[(int)pageIndex];
        ClearContentOutlines(slot);

        if (!_contentEditMode || !_pageContent.TryGetValue(pageIndex, out var state) || state.Content is not { } content)
        {
            return;
        }

        var page = _session.Pages[(int)pageIndex];
        var scale = slot.Scale;
        foreach (var run in content.TextRuns)
        {
            var outline = new Rectangle
            {
                Width = Math.Max(1, run.Bounds.Width * scale),
                Height = Math.Max(1, run.Bounds.Height * scale),
                StrokeThickness = 1,
                Stroke = new SolidColorBrush(run.IsEditable
                    ? global::Windows.UI.Color.FromArgb(120, 40, 120, 235)
                    : global::Windows.UI.Color.FromArgb(90, 130, 130, 130)),
                IsHitTestVisible = false,
            };
            if (!run.IsEditable)
            {
                outline.StrokeDashArray = [2, 2];
            }

            Canvas.SetLeft(outline, run.Bounds.X * scale);
            Canvas.SetTop(outline, (page.HeightPt - run.Bounds.Y - run.Bounds.Height) * scale);
            slot.Content.Children.Add(outline);
        }
    }

    /// <summary>
    /// Removes one page's outlines, leaving the inline editor — the one child
    /// of this canvas that is not a rectangle — where it is.
    /// </summary>
    private static void ClearContentOutlines(PageSlot slot)
    {
        for (var index = slot.Content.Children.Count - 1; index >= 0; index--)
        {
            if (slot.Content.Children[index] is Rectangle)
            {
                slot.Content.Children.RemoveAt(index);
            }
        }
    }

    /// <summary>The pages the viewport covers right now, or an empty run before the first layout.</summary>
    private PageWindow VisiblePageWindow() =>
        _slots.Count == 0 ? PageWindow.Empty : PageWindow.Resolve(_spans, PageScroller.VerticalOffset, PageScroller.ViewportHeight);

    /// <summary>Drops the outlines and the open editor, leaving the parsed content alone.</summary>
    private void ClearContentEditVisuals()
    {
        CancelContentEditor();
        foreach (var slot in _slots)
        {
            slot.Content.Children.Clear();
        }
    }

    /// <summary>
    /// Drops everything this mode holds — called when a new document replaces
    /// the session, and after an undo or redo.
    /// </summary>
    /// <remarks>
    /// The parsed content goes because its ids belong to the bytes of the
    /// document being replaced. The pending text goes with it: after an undo
    /// nobody here knows which command moved, so the honest answer is to read
    /// the page again rather than keep showing text that may no longer be
    /// queued.
    /// </remarks>
    private void ResetContentEditState()
    {
        ClearContentEditVisuals();
        _pageContent.Clear();
        _pendingRunText.Clear();
        _contentEditedPages.Clear();
    }

    /// <summary>
    /// Leaves the mode and drops everything it holds, without committing the
    /// open editor — for a new document arriving, where committing would send
    /// the previous document's text into whatever opened.
    /// </summary>
    private void ResetContentEditMode()
    {
        _contentEditMode = false;
        ContentEditButton.IsChecked = false;
        ResetContentEditState();
    }
}
