using Microsoft.UI.Xaml;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;

namespace Pdf.Windows;

/// <summary>
/// Add watermark: stamping the same text, or the same picture, across the
/// pages the reader picks, from the Edit PDF page's "Pages" card. The dialog
/// itself is in <c>MainWindow.WatermarkDialog.cs</c>.
/// </summary>
/// <remarks>
/// <para>
/// A dialog rather than an armed canvas click like Insert text, for the
/// reason the GTK shell gives: a watermark has no position to pick — the core
/// centres it on every page it names — and the one thing it does need is a
/// page list, which no single click can give.
/// </para>
/// <para>
/// The stamp is recorded from inside the dialog's primary button, so a
/// refusal — blank text, no picture, a bad range, a character Helvetica cannot
/// show, a file that is not a picture —
/// is shown under the fields and the reader corrects it in place, the way
/// Export images handles a bad range.
/// </para>
/// </remarks>
public sealed partial class MainWindow
{
    private bool _addingWatermark;

    private async void AddWatermarkButton_Click(object sender, RoutedEventArgs e)
    {
        if (_addingWatermark || _isBusy || _dialogOpen || _organizing || _session is not { PageCount: > 0 } session) return;
        if (!session.ContentEditingAllowed)
        {
            AnnotationStatus.Text = "This document does not permit content changes.";
            return;
        }
        _addingWatermark = true;
        try
        {
            // A retype still open in the editor is committed first, so the
            // stamp lands after it in the log and one undo takes only the stamp.
            await SettleContentEditorForHistoryAsync();
            if (_session?.SessionId != session.SessionId) return;
            var added = await AskWatermarkAsync(session);
            if (_session?.SessionId != session.SessionId) return;
            if (added is null)
            {
                AnnotationStatus.Text = "Watermark cancelled.";
                return;
            }
            await AfterWatermarkAsync(added);
        }
        finally
        {
            _addingWatermark = false;
        }
    }

    /// <summary>
    /// What a recorded stamp made stale: the pages it covers are re-rendered
    /// now and remembered for undo, which re-renders every content-edited page.
    /// </summary>
    private async Task AfterWatermarkAsync(WatermarkAdded added)
    {
        foreach (var pageIndex in added.Pages)
        {
            _contentEditedPages.Add(pageIndex);
            _pageContent.Remove(pageIndex);
            InvalidatePageCharacters(pageIndex);
            if (pageIndex < _slots.Count)
            {
                ClearContentOutlines(_slots[(int)pageIndex]);
                _slots[(int)pageIndex].Render.DropBitmap();
                _slots[(int)pageIndex].Tiles.Clear();
            }
        }
        // Once, not per page: a stamp on every page of a long document would
        // otherwise recompute the viewport once for each of them.
        UpdateViewport(intermediate: false);
        RedrawAnnotations();

        if (!added.Preview.IsSuccess)
        {
            // Recorded and undoable all the same; only the preview is behind.
            await RefreshAnnotationStateAsync();
            AnnotationStatus.Text = added.Preview.Error!.Message;
            return;
        }
        _annotationState = added.Preview.Value;
        UpdateAnnotationControls(_annotationState);
        AnnotationStatus.Text = WatermarkWording.Summary(added.Pages.Count);
    }
}
