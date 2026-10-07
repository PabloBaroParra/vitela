using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Media.Imaging;
using Microsoft.UI.Xaml.Shapes;
using Pdf.Windows.Facade;
using Pdf.Windows.Viewer;
using System.Runtime.InteropServices.WindowsRuntime;
using Windows.Storage.Streams;

namespace Pdf.Windows;

/// <summary>WinUI pointer adapter for the core-owned annotation edit log.</summary>
public sealed partial class MainWindow
{
    private static readonly AnnotationColor DefaultAnnotationColor = new(255, 220, 0);
    private static readonly SolidColorBrush HandleBrush = new(global::Windows.UI.Color.FromArgb(255, 26, 89, 217));
    private static readonly Corner[] AllCorners = [Corner.BottomLeft, Corner.BottomRight, Corner.TopLeft, Corner.TopRight];
    /// <summary>How far Nudge shifts the selection, in PDF points — matches the Linux shell's NUDGE_PT.</summary>
    private const double NudgePt = 12.0;
    /// <summary>How much Grow enlarges the selection — matches the Linux shell's RESIZE_FACTOR.</summary>
    private const double GrowFactor = 1.25;
    /// <summary>Floor for each side of a dragged rect — matches the Linux shell's MIN_TRACED_PT.</summary>
    private const double MinTracedPt = 4.0;
    /// <summary>Corner-handle size and grab radius in screen pixels — matches the Linux shell's HANDLE_PX.</summary>
    private const double HandleReachPx = 8.0;

    private AnnotationKind? _armedAnnotation;
    private AnnotationState? _annotationState;
    private ulong? _selectedAnnotationId;
    private PointerDrag? _pointerDrag;
    private readonly StampPreviewCache<BitmapImage> _stampPreviews = new();

    private void HighlightButton_Click(object sender, RoutedEventArgs e) => Arm(HighlightButton.IsChecked == true ? AnnotationKind.Highlight : null);
    private void UnderlineButton_Click(object sender, RoutedEventArgs e) => Arm(UnderlineButton.IsChecked == true ? AnnotationKind.Underline : null);
    private void StrikeoutButton_Click(object sender, RoutedEventArgs e) => Arm(StrikeoutButton.IsChecked == true ? AnnotationKind.Strikeout : null);
    private void ShapeButton_Click(object sender, RoutedEventArgs e) => Arm(ShapeButton.IsChecked == true ? AnnotationKind.Shape : null);
    private void InkButton_Click(object sender, RoutedEventArgs e) => Arm(InkButton.IsChecked == true ? AnnotationKind.Ink : null);
    private void NoteButton_Click(object sender, RoutedEventArgs e) => Arm(NoteButton.IsChecked == true ? AnnotationKind.TextNote : null);
    private void StampButton_Click(object sender, RoutedEventArgs e) => Arm(StampButton.IsChecked == true ? AnnotationKind.Stamp : null);
    private void PointerButton_Click(object sender, RoutedEventArgs e) => Arm(null);
    private async void UndoButton_Click(object sender, RoutedEventArgs e) => await ApplyHistoryAndMetadataAsync(undo: true);
    private async void RedoButton_Click(object sender, RoutedEventArgs e) => await ApplyHistoryAndMetadataAsync(undo: false);

    private async Task ApplyHistoryAndMetadataAsync(bool undo)
    {
        await ApplyHistoryAsync(undo);
        await RefreshDocumentInfoAsync();
        await RefreshFormFieldsAsync();
    }

    private async void NudgeButton_Click(object sender, RoutedEventArgs e)
    {
        if (_selectedAnnotationId is { } id) await ApplyEditAsync(new PdfCoreEdit.Move(id, NudgePt, NudgePt));
    }

    private async void GrowButton_Click(object sender, RoutedEventArgs e)
    {
        if (_selectedAnnotationId is not { } id) return;
        var selected = _annotationState?.Annotations.LastOrDefault(annotation => annotation.Id == id);
        if (selected?.Rect is not { } rect) return;
        var grown = new PdfCoreRect(rect.X, rect.Y, rect.Width * GrowFactor, rect.Height * GrowFactor);
        await ApplyEditAsync(new PdfCoreEdit.Resize(id, grown));
    }

    private async void DeleteAnnotationButton_Click(object sender, RoutedEventArgs e) => await DeleteSelectedAnnotationAsync();

    /// <summary>
    /// Delete removes the selected annotation, mirroring the Linux shell's
    /// <c>win.delete-annotation</c> accel. Two things keep it from stealing the
    /// key from the search box: the accelerator hangs off
    /// <see cref="DeleteAnnotationButton"/>, which is disabled whenever nothing
    /// is selected — the same guard the Linux comment relies on — and
    /// <see cref="TextInputHasFocus"/> covers what that leaves open, an
    /// annotation selected while the caret sits in a text field.
    /// </summary>
    private async void DeleteAnnotation_Invoked(KeyboardAccelerator sender, KeyboardAcceleratorInvokedEventArgs args)
    {
        if (TextInputHasFocus())
        {
            args.Handled = false;
            return;
        }

        args.Handled = true;
        await DeleteSelectedAnnotationAsync();
    }

    private async Task DeleteSelectedAnnotationAsync()
    {
        if (_annotationState?.EditingAllowed != true || _selectedAnnotationId is not { } id) return;
        await ApplyEditAsync(new PdfCoreEdit.Remove(id));
        _selectedAnnotationId = null;
    }

    private void Arm(AnnotationKind? kind)
    {
        if (_annotationState?.EditingAllowed != true || _isBusy || _organizing)
        {
            SyncAnnotationToolButtons();
            return;
        }
        StopPlacingFormField();
        // One mode owns a page click at a time: arming a tool leaves content
        // editing, exactly as arming content editing disarms the tools.
        SetContentEditMode(false);
        _armedAnnotation = kind;
        _armedStampImage = null; // choosing a tool by hand replaces an armed drawn signature
        SyncAnnotationToolButtons();
        AnnotationStatus.Text = kind is null ? "Pointer mode." : $"{kind} armed. Drag on a page to place it.";
    }

    /// <summary>
    /// Reflects the armed tool without creating a second owner of the mode.
    /// Click handlers accept user changes; programmatic IsChecked updates do
    /// not re-enter them when another mode or a completed placement disarms it.
    /// </summary>
    private void SyncAnnotationToolButtons()
    {
        if (_armedAnnotation != AnnotationKind.Stamp) _armedStampImage = null;
        HighlightButton.IsChecked = _armedAnnotation == AnnotationKind.Highlight;
        UnderlineButton.IsChecked = _armedAnnotation == AnnotationKind.Underline;
        StrikeoutButton.IsChecked = _armedAnnotation == AnnotationKind.Strikeout;
        ShapeButton.IsChecked = _armedAnnotation == AnnotationKind.Shape;
        InkButton.IsChecked = _armedAnnotation == AnnotationKind.Ink;
        NoteButton.IsChecked = _armedAnnotation == AnnotationKind.TextNote;
        StampButton.IsChecked = _armedAnnotation == AnnotationKind.Stamp;
    }

    private void ConnectAnnotationPointer(PageSlot slot, int pageIndex)
    {
        // Text selection is tried only once annotations refuse the gesture —
        // an armed tool placing an annotation, or a press landing on an
        // existing annotation's handle or body, is aimed at the annotation,
        // not at the text underneath it. See `MainWindow.Selection.cs`.
        slot.Annotations.PointerPressed += (_, args) =>
        {
            if (_isBusy || _dialogOpen) return;
            if (BeginFormFieldPlacement(slot, pageIndex, args)) return;
            // Content editing is asked first, and answers for the whole
            // gesture when armed: it is a mode, not a tool competing for the
            // same click. See `MainWindow.ContentEdit.cs`.
            if (BeginContentEditPointer(slot, pageIndex, args)) return;
            if (!BeginAnnotationPointer(slot, pageIndex, args)) BeginTextSelection(slot, pageIndex, args);
        };
        slot.Annotations.PointerMoved += (_, args) =>
        {
            if (_formEditMode)
            {
                ContinueFormGesture(pageIndex, ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position));
                return;
            }
            if (_contentEditMode)
            {
                ContinueContentImageGesture(pageIndex, ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position));
                return;
            }
            if (!ContinueAnnotationPointer(slot, pageIndex, args)) ContinueTextSelection(slot, pageIndex, args);
        };
        slot.Annotations.PointerReleased += async (_, args) =>
        {
            if (await EndFormFieldPlacementAsync(slot, pageIndex, args)) return;
            if (_contentEditMode)
            {
                await EndContentImageGestureAsync(slot, pageIndex, args);
                return;
            }
            if (!await EndAnnotationPointerAsync(slot, pageIndex, args)) EndTextSelection(slot, pageIndex, args);
        };
        slot.Annotations.PointerCaptureLost += (_, _) =>
        {
            CancelPagePointerGesture(pageIndex);
        };
        slot.Annotations.PointerCanceled += (_, args) =>
        {
            CancelPagePointerGesture(pageIndex);
            slot.Annotations.ReleasePointerCapture(args.Pointer);
        };
        ConnectFileDrop(slot, pageIndex);
    }

    /// <summary>
    /// Capture loss can replace pointer-up. Discard the uncommitted annotation
    /// or field gesture and freeze text selection at its last sampled position.
    /// Normal pointer-up clears its live state before releasing capture.
    /// </summary>
    private void CancelPagePointerGesture(int pageIndex)
    {
        if (_pointerDrag?.PageIndex == pageIndex)
        {
            _pointerDrag = null;
            RedrawAnnotations();
        }
        CancelFormGesture(pageIndex);
        CancelContentImageGesture(pageIndex);
        if (_textSelection?.PageIndex == pageIndex) _textDragActive = false;
    }

    /// <summary>Returns whether the press was claimed as an annotation gesture.</summary>
    private bool BeginAnnotationPointer(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_session is null) return false;
        // Annotation editing being refused does not claim the gesture: the
        // document may still permit text selection (placing an annotation and
        // extracting text are gated by different permission bits).
        if (_annotationState?.EditingAllowed != true) return false;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);

        bool claimed;
        if (_armedAnnotation is { } kind)
        {
            _pointerDrag = new PointerDrag(pageIndex, point, null, kind, null)
            {
                Trace = kind == AnnotationKind.Ink ? new InkPointerTrace(point) : null,
            };
            claimed = true;
        }
        else if (TryGrabSelected(pageIndex, point, slot.Scale, out var grabbed))
        {
            _pointerDrag = grabbed;
            claimed = true;
        }
        else
        {
            // A press that hits no annotation is a deselection, not a claim:
            // the gesture goes on to select text, same as a press that never
            // reached here because editing was refused.
            var hit = HitTest((uint)pageIndex, point);
            _selectedAnnotationId = hit?.Id;
            _pointerDrag = hit is not null ? new PointerDrag(pageIndex, point, hit, null, null) : null;
            claimed = hit is not null;
        }

        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        if (claimed) slot.Annotations.CapturePointer(args.Pointer);
        return claimed;
    }

    /// <summary>
    /// The selected annotation gets first refusal on a press, so its resize
    /// handles stay reachable even where another annotation overlaps them —
    /// mirrors the Linux shell's <c>begin_annotation_drag</c>. A corner near an
    /// <c>Ink</c> stroke's bounding box only starts a move, never a resize: Ink
    /// has no rect to replace, the same restriction <c>supports_resize</c> encodes.
    /// </summary>
    private bool TryGrabSelected(int pageIndex, AnnotationPoint point, double scale, out PointerDrag drag)
    {
        drag = default;
        if (_selectedAnnotationId is not { } id) return false;
        var selected = _annotationState?.Annotations.LastOrDefault(annotation => annotation.Id == id && annotation.PageIndex == pageIndex);
        if (selected is null || AnnotationBounds(selected) is not { } bounds) return false;

        var reach = HandleReachPx / scale;
        if (selected.Rect is not null && CornerAt(bounds, point, reach) is { } corner)
        {
            drag = new PointerDrag(pageIndex, point, selected, null, corner);
            return true;
        }
        if (Contains(bounds, point))
        {
            drag = new PointerDrag(pageIndex, point, selected, null, null);
            return true;
        }
        return false;
    }

    /// <summary>Returns whether an annotation drag was in flight for this page.</summary>
    private bool ContinueAnnotationPointer(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_pointerDrag is not { PageIndex: var dragPage } drag || dragPage != pageIndex) return false;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        _pointerDrag = drag with { Current = point };
        drag.Trace?.Append(point);
        RedrawAnnotations();
        return true;
    }

    /// <summary>Returns whether an annotation drag was in flight for this page.</summary>
    private async Task<bool> EndAnnotationPointerAsync(PageSlot slot, int pageIndex, PointerRoutedEventArgs args)
    {
        if (_pointerDrag is not { PageIndex: var dragPage } drag || dragPage != pageIndex) return false;
        var point = ToPdf(slot, pageIndex, args.GetCurrentPoint(slot.Annotations).Position);
        drag.Trace?.Append(point);
        var completed = drag with { Current = point };
        // ReleasePointerCapture can synchronously raise PointerCaptureLost.
        // Detach the completed gesture first so cancellation cannot discard it.
        _pointerDrag = null;
        slot.Annotations.ReleasePointerCapture(args.Pointer);
        if (completed.Tool is { } tool)
        {
            await CommitPlacementAsync(pageIndex, tool, completed);
            _armedAnnotation = null;
            SyncAnnotationToolButtons();
        }
        else if (completed.HandleCorner is { } corner && completed.Annotation?.Rect is { } bounds)
        {
            var resized = ResizedRect(bounds, corner, completed.Current);
            if (resized != bounds) await ApplyEditAsync(new PdfCoreEdit.Resize(completed.Annotation.Id, new PdfCoreRect(resized.X, resized.Y, resized.Width, resized.Height)));
        }
        else if (completed.Annotation is { } annotation)
        {
            var dx = completed.Current.X - completed.Origin.X;
            var dy = completed.Current.Y - completed.Origin.Y;
            if (dx != 0 || dy != 0) await ApplyEditAsync(new PdfCoreEdit.Move(annotation.Id, dx, dy));
        }
        RedrawAnnotations();
        return true;
    }

    /// <summary>
    /// Ink is a traced polyline, not a corner-to-corner rect — a tap leaves a
    /// single point, which is not a stroke, so it is reported rather than saved
    /// as an invisible annotation (mirrors the Linux shell's <c>INK_NEEDS_A_DRAG</c>).
    /// </summary>
    private async Task CommitPlacementAsync(int pageIndex, AnnotationKind tool, PointerDrag completed)
    {
        if (tool == AnnotationKind.Ink)
        {
            if (completed.Trace?.Points is not { Count: >= 2 } points)
            {
                AnnotationStatus.Text = "Ink needs a drag, not a tap.";
                return;
            }
            var color = new PdfCoreColor(DefaultAnnotationColor.R, DefaultAnnotationColor.G, DefaultAnnotationColor.B);
            await ApplyEditAsync(new PdfCoreEdit.Add(PdfCoreAnnotationKind.Ink, (uint)pageIndex, new PdfCoreRect(0, 0, 0, 0), color, [.. points.Select(p => new PdfCorePoint(p.X, p.Y))]));
            return;
        }
        var rect = NormalizedRect(completed.Origin, completed.Current, tool);
        if (tool == AnnotationKind.Stamp)
        {
            // A drawn signature is placed where the click landed; otherwise the reader picks a file.
            if (_armedStampImage is { } signature) await PlaceArmedStampAsync((uint)pageIndex, completed.Origin, signature);
            else await PlaceImageStampAsync((uint)pageIndex, rect);
            return;
        }
        if (tool == AnnotationKind.TextNote)
        {
            await PlaceTextNoteAsync((uint)pageIndex, rect);
            return;
        }
        await ApplyEditAsync(new PdfCoreEdit.Add((PdfCoreAnnotationKind)tool, (uint)pageIndex, rect, new PdfCoreColor(DefaultAnnotationColor.R, DefaultAnnotationColor.G, DefaultAnnotationColor.B)));
    }

    /// <summary>
    /// Stamp does not go through <see cref="PdfCoreEdit"/> — <c>insert_image_stamp</c>
    /// is its own FFI entrypoint because it carries image bytes, not a small
    /// value type.
    /// </summary>
    private async Task InsertStampFromImageBytesAsync(string sessionId, uint pageIndex, PdfCoreRect rect, byte[] imageBytes)
    {
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        if (_annotationState?.EditingAllowed != true) return;
        if (!ImageStampInput.HasSupportedSignature(imageBytes))
        {
            AnnotationStatus.Text = "The image is not a supported PNG or JPEG.";
            return;
        }

        var before = _annotationState.Annotations;
        var result = await _facade.InsertStampAsync(sessionId, pageIndex, imageBytes, rect);
        if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        _annotationState = result.Value!;
        if (StampPreviewReconciliation.InsertedStampId(before, _annotationState.Annotations) is { } annotationId)
        {
            // A stamp arrives where the user pointed, at a default size they
            // will usually want to move or resize — so it lands selected, with
            // handles up and Nudge/Grow/Delete live, exactly as the Linux shell
            // does in `stamp_from_image_bytes`. Set before the controls refresh
            // below, which reads the selection.
            _selectedAnnotationId = annotationId;
            try
            {
                var preview = await DecodeStampPreviewAsync(imageBytes);
                if (ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) _stampPreviews.Set(sessionId, annotationId, preview);
            }
            catch (Exception)
            {
                // A valid PDF stamp can still lack a local preview; keep the rect fallback.
            }
        }
        if (!ImageStampInput.SessionMatches(sessionId, _session?.SessionId)) return;
        AnnotationStatus.Text = "Image stamp added. Changes are pending save.";
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
    }

    private static async Task<BitmapImage> DecodeStampPreviewAsync(byte[] imageBytes)
    {
        using var stream = new InMemoryRandomAccessStream();
        await stream.WriteAsync(imageBytes.AsBuffer());
        stream.Seek(0);
        var preview = new BitmapImage();
        await preview.SetSourceAsync(stream);
        return preview;
    }

    private async Task ApplyEditAsync(PdfCoreEdit edit)
    {
        if (_session is null) return;
        var result = await _facade.EditAnnotationAsync(_session.SessionId, edit);
        if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
        _annotationState = result.Value!;
        AnnotationStatus.Text = "Annotation edited. Changes are pending save.";
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
    }

    private async Task ApplyHistoryAsync(bool undo)
    {
        if (_session is null) return;
        await _formFillTail;
        if (_session is null) return;
        // The toolbar stays live while the inline editor has focus, so a
        // history step can arrive with a box open over the run it is about to
        // move. Resolve the box first — see
        // `SettleContentEditorForHistoryAsync` for what happens otherwise.
        // A no-op when no editor is open, which is every annotation-only
        // session.
        await SettleContentEditorForHistoryAsync();
        if (_session is null) return;
        var result = await (undo ? _facade.UndoAsync(_session.SessionId) : _facade.RedoAsync(_session.SessionId));
        if (!result.IsSuccess) { AnnotationStatus.Text = result.Error!.Message; return; }
        var state = result.Value!;
        _annotationState = state;
        if (_selectedAnnotationId is { } id && !state.Annotations.Any(annotation => annotation.Id == id)) _selectedAnnotationId = null;
        AnnotationStatus.Text = undo ? "Edit undone. Changes are pending save." : "Edit redone. Changes are pending save.";
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
        // A step that moved, turned or restored a page changes the whole
        // layout; the rebuild re-renders every page, so the per-page
        // catch-up below has nothing left to add.
        if (_pagesEdited)
        {
            await SyncPagesAfterHistoryAsync();
            return;
        }

        // A step that moved a page-content edit changes what the PDF itself
        // paints, and the facade has already rebuilt the preview for it. Only
        // the pages on screen are left to catch up — and only a session that
        // has touched content pays for it.
        if (_contentEditedPages.Count > 0)
        {
            ForgetPendingContentText();
            InvalidateContentEditedPages();
        }

        // A filled field is painted by the PDF too, so the same holds for it.
        if (_filledFieldPages.Count > 0)
        {
            InvalidateFilledFieldPages();
        }
    }

    private async Task RefreshAnnotationStateAsync()
    {
        if (_session is null) return;
        var result = await _facade.AnnotationStateAsync(_session.SessionId);
        if (!result.IsSuccess || _session.SessionId != result.Value!.SessionId) return;
        _annotationState = result.Value;
        UpdateAnnotationControls(_annotationState);
        RedrawAnnotations();
    }

    private void UpdateAnnotationControls(AnnotationState? state)
    {
        SyncAnnotationToolButtons();
        // Organizing hides the pages these tools draw on; only history stays live.
        var enabled = state?.EditingAllowed == true && _session is { PageCount: > 0 } && !_organizing && !_isBusy;
        var selected = _selectedAnnotationId is { } id
            ? state?.Annotations.LastOrDefault(annotation => annotation.Id == id)
            : null;
        // Its own permission bit, and its own answer: a document can allow
        // annotations and refuse content changes, or the reverse. It follows
        // `state` only for the part they share — a blanked toolbar means the
        // shell is busy or has no document, and nothing may be armed then.
        ContentEditButton.IsEnabled = state is not null && _session is { ContentEditingAllowed: true, PageCount: > 0 } && !_organizing && !_isBusy;
        ResizeImageButton.IsEnabled = ContentEditButton.IsEnabled && !_isBusy;
        MoveTextButton.IsEnabled = ContentEditButton.IsEnabled && !_isBusy;
        InsertTextButton.IsEnabled = ContentEditButton.IsEnabled && !_isBusy;
        MoveImageButton.IsEnabled = ContentEditButton.IsEnabled && !_isBusy;
        InsertImageButton.IsEnabled = ContentEditButton.IsEnabled && !_isBusy;
        // Delete and Replace image follow the canvas selection, not just the
        // permission — see UpdateImageCard, which this reaches.
        UpdateEditPanelAvailability();
        HighlightButton.IsEnabled = enabled;
        UnderlineButton.IsEnabled = enabled;
        StrikeoutButton.IsEnabled = enabled;
        ShapeButton.IsEnabled = enabled;
        InkButton.IsEnabled = enabled;
        NoteButton.IsEnabled = enabled;
        StampButton.IsEnabled = enabled;
        PointerButton.IsEnabled = enabled;
        PreviousAnnotationButton.IsEnabled = enabled && selected is not null;
        NextAnnotationButton.IsEnabled = state is { Annotations.Count: > 0 } && !_organizing && !_isBusy;
        ReadNoteButton.IsEnabled = selected?.Kind == AnnotationKind.TextNote && !_organizing && !_isBusy;
        UndoButton.IsEnabled = state?.CanUndo == true && !_isBusy;
        RedoButton.IsEnabled = state?.CanRedo == true && !_isBusy;
        DeleteAnnotationButton.IsEnabled = enabled && selected is not null;
        NudgeButton.IsEnabled = enabled && selected is not null;
        MoveAnnotationButton.IsEnabled = enabled && !_isBusy && selected is not null && AnnotationBounds(selected) is not null;
        GrowButton.IsEnabled = enabled && selected?.Rect is not null;
        ResizeAnnotationButton.IsEnabled = GrowButton.IsEnabled && !_isBusy;
        var restyleEnabled = enabled && selected is not null && SupportsRestyle(selected.Kind);
        AnnotationColorButton.IsEnabled = restyleEnabled;
        UpdateFormToolbar();
        UpdateSigningControls();
        UpdateDrawSignatureControls();
    }

    /// <summary>Mirrors the Linux shell's <c>supports_restyle</c>: only kinds carrying a color field can be restyled.</summary>
    private static bool SupportsRestyle(AnnotationKind kind) =>
        kind is AnnotationKind.Highlight or AnnotationKind.Underline or AnnotationKind.Strikeout or AnnotationKind.Ink or AnnotationKind.Shape;

    private Annotation? HitTest(uint pageIndex, AnnotationPoint point) => _annotationState?.Annotations
        .Where(annotation => annotation.PageIndex == pageIndex && AnnotationBounds(annotation) is not null)
        .LastOrDefault(annotation => Contains(AnnotationBounds(annotation)!, point));

    /// <summary>
    /// The area an annotation occupies, for hit-testing, handles, and move —
    /// rect-based kinds report their own rect; <c>Ink</c> has none, so its
    /// bounding box is derived from its points. Mirrors the Linux shell's
    /// <c>bounds</c> (distinct from <c>resize_rect</c>: Ink is hit-testable and
    /// movable through this, but still not resizable — see <see cref="TryGrabSelected"/>).
    /// </summary>
    private static AnnotationRect? AnnotationBounds(Annotation annotation)
    {
        if (annotation.Rect is { } rect) return rect;
        if (annotation.Points.Count == 0) return null;
        var minX = annotation.Points.Min(point => point.X);
        var minY = annotation.Points.Min(point => point.Y);
        var maxX = annotation.Points.Max(point => point.X);
        var maxY = annotation.Points.Max(point => point.Y);
        return new AnnotationRect(minX, minY, maxX - minX, maxY - minY);
    }

    private static bool Contains(AnnotationRect rect, AnnotationPoint point) => point.X >= rect.X && point.X <= rect.X + rect.Width && point.Y >= rect.Y && point.Y <= rect.Y + rect.Height;
    private static PdfCoreRect NormalizedRect(AnnotationPoint origin, AnnotationPoint current, AnnotationKind kind)
    {
        var width = Math.Max(MinTracedPt, Math.Abs(current.X - origin.X));
        var height = kind is AnnotationKind.Underline or AnnotationKind.Strikeout ? 2 : Math.Max(MinTracedPt, Math.Abs(current.Y - origin.Y));
        return new PdfCoreRect(Math.Min(origin.X, current.X), kind is AnnotationKind.Underline or AnnotationKind.Strikeout ? origin.Y : Math.Min(origin.Y, current.Y), width, height);
    }

    /// <summary>Where a corner sits, in PDF page space — mirrors the Linux shell's <c>corner_point</c>.</summary>
    private static AnnotationPoint CornerPoint(AnnotationRect rect, Corner corner) => corner switch
    {
        Corner.BottomLeft => new AnnotationPoint(rect.X, rect.Y),
        Corner.BottomRight => new AnnotationPoint(rect.X + rect.Width, rect.Y),
        Corner.TopLeft => new AnnotationPoint(rect.X, rect.Y + rect.Height),
        Corner.TopRight => new AnnotationPoint(rect.X + rect.Width, rect.Y + rect.Height),
        _ => throw new ArgumentOutOfRangeException(nameof(corner)),
    };

    /// <summary>The corner whose handle is under <paramref name="point"/>, if any — mirrors <c>corner_at</c>.</summary>
    private static Corner? CornerAt(AnnotationRect rect, AnnotationPoint point, double reach)
    {
        foreach (var corner in AllCorners)
        {
            var at = CornerPoint(rect, corner);
            if (Math.Abs(point.X - at.X) <= reach && Math.Abs(point.Y - at.Y) <= reach) return corner;
        }
        return null;
    }

    private static Corner Opposite(Corner corner) => corner switch
    {
        Corner.BottomLeft => Corner.TopRight,
        Corner.BottomRight => Corner.TopLeft,
        Corner.TopLeft => Corner.BottomRight,
        Corner.TopRight => Corner.BottomLeft,
        _ => throw new ArgumentOutOfRangeException(nameof(corner)),
    };

    /// <summary>
    /// The rect a resize drag produces: the grabbed corner follows the pointer,
    /// the opposite corner stays put, and the result is normalized so dragging a
    /// corner past its opposite flips the rect rather than inverting it — mirrors
    /// the Linux shell's <c>resized_rect</c>.
    /// </summary>
    private static AnnotationRect ResizedRect(AnnotationRect rect, Corner corner, AnnotationPoint point)
    {
        var anchor = CornerPoint(rect, Opposite(corner));
        var width = Math.Max(MinTracedPt, Math.Abs(point.X - anchor.X));
        var height = Math.Max(MinTracedPt, Math.Abs(point.Y - anchor.Y));
        return new AnnotationRect(Math.Min(anchor.X, point.X), Math.Min(anchor.Y, point.Y), width, height);
    }

    private static AnnotationRect MovedRect(AnnotationRect rect, AnnotationPoint origin, AnnotationPoint current) =>
        new(rect.X + (current.X - origin.X), rect.Y + (current.Y - origin.Y), rect.Width, rect.Height);

    private void RedrawAnnotations()
    {
        foreach (var slot in _slots) slot.Annotations.Children.Clear();
        if (_annotationState is null) return;

        var draggedId = _pointerDrag is { Tool: null, Annotation: { } dragged } ? dragged.Id : (ulong?)null;
        foreach (var annotation in _annotationState.Annotations)
        {
            if (annotation.PageIndex >= _slots.Count || annotation.Id == draggedId) continue;
            var slot = _slots[(int)annotation.PageIndex];
            if (annotation.Kind == AnnotationKind.Ink)
            {
                DrawInkStroke(slot, annotation.PageIndex, annotation.Points, ResolveDisplayColor(annotation), selected: _selectedAnnotationId == annotation.Id);
            }
            else if (annotation.Rect is not null)
            {
                DrawAnnotationShape(slot, annotation.PageIndex, annotation, annotation.Rect, ResolveDisplayColor(annotation), selected: _selectedAnnotationId == annotation.Id);
            }
        }

        if (_pointerDrag is { Tool: null, Annotation: { } dragging } move)
        {
            var dx = move.Current.X - move.Origin.X;
            var dy = move.Current.Y - move.Origin.Y;
            if (dragging.Kind == AnnotationKind.Ink)
            {
                var moved = dragging.Points.Select(point => new AnnotationPoint(point.X + dx, point.Y + dy)).ToList();
                DrawInkStroke(_slots[move.PageIndex], (uint)move.PageIndex, moved, ResolveDisplayColor(dragging), selected: true);
            }
            else if (dragging.Rect is { } bounds)
            {
                var rect = move.HandleCorner is { } corner ? ResizedRect(bounds, corner, move.Current) : MovedRect(bounds, move.Origin, move.Current);
                DrawAnnotationShape(_slots[move.PageIndex], (uint)move.PageIndex, dragging, rect, ResolveDisplayColor(dragging), selected: true);
            }
        }
        else if (_selectedAnnotationId is { } selectedId
            && _annotationState.Annotations.LastOrDefault(annotation => annotation.Id == selectedId) is { } selectedAnnotation
            && AnnotationBounds(selectedAnnotation) is { } selectedBounds
            && selectedAnnotation.PageIndex < _slots.Count)
        {
            DrawHandles(_slots[(int)selectedAnnotation.PageIndex], selectedAnnotation.PageIndex, selectedBounds);
        }

        if (_pointerDrag is { Tool: { } tool } toolDrag)
        {
            var slot = _slots[toolDrag.PageIndex];
            if (tool == AnnotationKind.Ink && toolDrag.Trace?.Points is { Count: > 0 } points)
            {
                DrawInkStroke(slot, (uint)toolDrag.PageIndex, points, GoldenrodAnnotationColor, selected: false);
            }
            else
            {
                var rect = NormalizedRect(toolDrag.Origin, toolDrag.Current, tool);
                var isRule = tool is AnnotationKind.Underline or AnnotationKind.Strikeout;
                var preview = new Rectangle { Stroke = isRule ? null : new SolidColorBrush(Colors.Goldenrod), Fill = isRule ? new SolidColorBrush(Colors.Goldenrod) : null, StrokeThickness = 2 };
                PlaceOverPage(preview, slot, toolDrag.PageIndex, new AnnotationRect(rect.X, rect.Y, rect.Width, rect.Height), minSide: 2);
                slot.Annotations.Children.Add(preview);
            }
        }
        DrawFormFieldSelection();
    }

    private static readonly AnnotationColor GoldenrodAnnotationColor = new(218, 165, 32);

    private static AnnotationColor ResolveDisplayColor(Annotation annotation) => annotation.Color ?? DefaultAnnotationColor;

    private void DrawAnnotationShape(PageSlot slot, uint pageIndex, Annotation annotation, AnnotationRect rect, AnnotationColor color, bool selected)
    {
        // The rect arrives from the caller rather than from the annotation, so
        // a stamp being dragged or resized is painted at the geometry under the
        // pointer, and every paint re-reads slot.Scale — which is what keeps
        // the image in step with a zoom.
        if (annotation.Kind == AnnotationKind.Stamp
            && _session is { } session
            && _stampPreviews.IsCurrent(session.SessionId)
            && _stampPreviews.TryGet(annotation.Id, out var preview))
        {
            // Sized upright and turned with the page: a saved stamp's
            // appearance turns with the page, so its preview must too.
            var image = new Image { Source = preview, Width = rect.Width * slot.Scale, Height = rect.Height * slot.Scale, Stretch = Stretch.Fill };
            PlaceUpright(image, slot, (int)pageIndex, rect);
            slot.Annotations.Children.Add(image);
            return;
        }
        var isRule = annotation.Kind is AnnotationKind.Underline or AnnotationKind.Strikeout;
        var brush = new SolidColorBrush(global::Windows.UI.Color.FromArgb(220, color.R, color.G, color.B));
        var shape = new Rectangle { Stroke = isRule ? null : brush, Fill = annotation.Kind == AnnotationKind.Highlight ? new SolidColorBrush(global::Windows.UI.Color.FromArgb(100, color.R, color.G, color.B)) : isRule ? brush : null, StrokeThickness = selected ? 3 : 2 };
        PlaceOverPage(shape, slot, (int)pageIndex, rect, minSide: 2);
        AttachNoteTooltip(shape, annotation);
        slot.Annotations.Children.Add(shape);
    }

    /// <summary>A freehand stroke as a polyline — <c>Ink</c> has no rect, only the traced points.</summary>
    private void DrawInkStroke(PageSlot slot, uint pageIndex, IReadOnlyList<AnnotationPoint> points, AnnotationColor color, bool selected)
    {
        if (points.Count < 2) return;
        var polyline = new Polyline
        {
            Stroke = new SolidColorBrush(global::Windows.UI.Color.FromArgb(220, color.R, color.G, color.B)),
            StrokeThickness = selected ? 3 : 2,
            StrokeLineJoin = PenLineJoin.Round,
            StrokeStartLineCap = PenLineCap.Round,
            StrokeEndLineCap = PenLineCap.Round,
        };
        foreach (var point in points)
        {
            var placed = PlaceOnPage(slot, (int)pageIndex, point);
            polyline.Points.Add(new global::Windows.Foundation.Point(placed.Left, placed.Top));
        }
        slot.Annotations.Children.Add(polyline);
    }

    /// <summary>
    /// Four fixed-size squares at the selected annotation's corners — sized in
    /// screen pixels rather than scaled with the page, so they stay grabbable
    /// however far the page is zoomed. Mirrors the Linux shell's <c>draw_handles</c>.
    /// </summary>
    private void DrawHandles(PageSlot slot, uint pageIndex, AnnotationRect rect)
    {
        foreach (var corner in AllCorners)
        {
            var point = PlaceOnPage(slot, (int)pageIndex, CornerPoint(rect, corner));
            var handle = new Rectangle { Width = HandleReachPx, Height = HandleReachPx, Fill = HandleBrush };
            Canvas.SetLeft(handle, point.Left - HandleReachPx / 2);
            Canvas.SetTop(handle, point.Top - HandleReachPx / 2);
            slot.Annotations.Children.Add(handle);
        }
    }

    /// <summary>Named by PDF-space position, where <c>y</c> grows upward — mirrors the Linux shell's <c>Corner</c>.</summary>
    private enum Corner { BottomLeft, BottomRight, TopLeft, TopRight }

    private readonly record struct PointerDrag(int PageIndex, AnnotationPoint Origin, Annotation? Annotation, AnnotationKind? Tool, Corner? HandleCorner)
    {
        public AnnotationPoint Current { get; init; } = Origin;
        /// <summary>Accumulated trace for an in-progress <c>Ink</c> placement; unused otherwise.</summary>
        public InkPointerTrace? Trace { get; init; }
    }
}
