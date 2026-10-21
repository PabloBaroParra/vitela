namespace Pdf.Windows.Facade;

/// <summary>
/// Add watermark: stamping the same text, or the same picture, across the
/// pages the reader picks — over the page content or behind it.
/// </summary>
/// <remarks>
/// <para>
/// The Windows twin of the GTK shell's <c>content_edit::watermark::record</c>.
/// <c>AddWatermark</c> is page content, painted into each page's content
/// stream at save time, so it answers to the content-edit permission and
/// needs the same preview refresh as every other content edit. However many
/// pages it covers, it is one entry in the core's log — one undo step.
/// </para>
/// <para>
/// Everything a reader can get wrong is refused before the log is touched:
/// blank text and an empty range here, in words; a bad range by the core's
/// own grammar; and text Helvetica cannot show, or a file that is not a PNG
/// or JPEG, by the core's dry run inside <see cref="IPdfCore.ApplyEdit"/>, so
/// a stamp that could not be written never reaches a save where it would take
/// every other queued edit down with it.
/// </para>
/// </remarks>
public sealed partial class PdfDocumentFacade
{
    /// <summary>The core's diagonal look, with opacity in the dialog's percent.</summary>
    public WatermarkLook DefaultWatermarkLook()
    {
        var look = _core.DefaultWatermark([], WatermarkLimits.DefaultText);
        return new WatermarkLook(look.FontSizePt, look.Opacity * 100, look.RotationDegrees, look.Layer);
    }

    /// <summary>
    /// The core's picture look — upright, half the page, behind the content —
    /// with opacity and size in the dialog's percent. Asked of the core with
    /// no picture: the look does not depend on one.
    /// </summary>
    public PictureWatermarkLook DefaultPictureWatermarkLook()
    {
        var look = _core.DefaultImageWatermark([], []);
        return new PictureWatermarkLook(look.Scale * 100, look.Opacity * 100, look.RotationDegrees, look.Layer);
    }

    /// <summary>
    /// Records one watermark over the pages <paramref name="request"/> names.
    /// A failure means nothing was recorded; a success carries the preview
    /// refresh separately, because the stamp stays recorded even when that
    /// refresh fails.
    /// </summary>
    public async Task<OperationResult<WatermarkAdded>> AddWatermarkAsync(string sessionId, WatermarkRequest request)
    {
        const string operation = "add_watermark";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            IReadOnlyList<uint> pages;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return Refuse("The document is no longer available.", PdfCoreError.DocumentNotFound);
                if (!_core.ContentEditingAllowed(session.Document))
                    return Refuse("This document does not permit content changes.", PdfCoreError.UnsupportedOperation);
                if (request.Mark is TextMark { Text: var typed } && string.IsNullOrWhiteSpace(typed))
                    return Refuse("Type the text to stamp on the pages.", PdfCoreError.UnsupportedOperation);
                if (request.Mark is PictureMark { Image.Length: 0 })
                    return Refuse("Choose a PNG or JPEG picture to stamp on the pages.", PdfCoreError.InvalidImage);

                var total = session.Document.PageCount;
                if (total == 0)
                    return Refuse("This document has no pages to stamp.", PdfCoreError.PageIndexOutOfBounds);
                if (request.Pages == WatermarkPages.Custom && string.IsNullOrWhiteSpace(request.PageRange))
                    return Refuse("Type which pages to stamp, for example 1-3,7.", PdfCoreError.InvalidPageSelection);
                pages = request.Pages switch
                {
                    WatermarkPages.All => [.. Enumerable.Range(0, (int)total).Select(page => (uint)page)],
                    // Clamped for the reason export gives: the first-visible
                    // readout can sit one past the end right after a deletion.
                    WatermarkPages.Current => [Math.Min(request.CurrentPage, total - 1)],
                    _ => _core.ParsePageSelection(request.PageRange, total),
                };

                _core.ApplyEdit(session.Document, StampFor(request, pages));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }
            var preview = await RefreshPreviewAsync(session, operation, null).ConfigureAwait(false);
            return OperationResult<WatermarkAdded>.Success(new WatermarkAdded(pages, preview));
        }
        // The core's own InvalidImage sentence is about "the image"; here the
        // reader picked a file, and that is the thing to change.
        catch (PdfCoreException error) when (error.Category == PdfCoreError.InvalidImage)
        {
            return Refuse("That file could not be read as a PNG or JPEG picture. Choose another one.", PdfCoreError.InvalidImage);
        }
        catch (PdfCoreException error) { return OperationResult<WatermarkAdded>.Failure(MapError(error, operation, sessionId, null)); }
        catch (Exception error) { return OperationResult<WatermarkAdded>.Failure(MapUnexpected(error, operation, sessionId, null)); }
        finally { _documentChangeGate.Release(); }

        OperationResult<WatermarkAdded> Refuse(string message, PdfCoreError category) =>
            OperationResult<WatermarkAdded>.Failure(CreateError(message, category, operation, sessionId, null));
    }

    /// <summary>
    /// The core's default look for the kind of mark, with only what the dialog
    /// asks for laid over it — the colour stays the core's.
    /// </summary>
    private PdfCoreEdit StampFor(WatermarkRequest request, IReadOnlyList<uint> pages) => request.Mark switch
    {
        PictureMark picture => new PdfCoreEdit.AddImageWatermark(_core.DefaultImageWatermark(pages, picture.Image) with
        {
            Scale = picture.ScalePercent / 100,
            Opacity = request.OpacityPercent / 100,
            RotationDegrees = request.RotationDegrees,
            Layer = request.Layer,
        }),
        TextMark text => new PdfCoreEdit.AddWatermark(_core.DefaultWatermark(pages, text.Text) with
        {
            FontSizePt = text.FontSizePt,
            Opacity = request.OpacityPercent / 100,
            RotationDegrees = request.RotationDegrees,
            Layer = request.Layer,
        }),
        _ => throw new ArgumentOutOfRangeException(nameof(request)),
    };
}
