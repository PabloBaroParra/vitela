namespace Pdf.Windows.Facade;

/// <summary>
/// What CTRL+C or CTRL+X put on the content clipboard (B25 T-209): a
/// self-contained one-page PDF and the box its items covered, PDF space. It
/// names nothing in the document it came from, so it stays valid after that
/// page — or that document — changes or closes.
/// </summary>
public sealed class ContentClip
{
    internal ContentClip(PdfCoreContentClip source) => Source = source;

    internal PdfCoreContentClip Source { get; }

    /// <summary>The copied items' union box; a paste puts its top-left corner at the cursor.</summary>
    public AnnotationRect Bounds => new(Source.Bbox.X, Source.Bbox.Y, Source.Bbox.Width, Source.Bbox.Height);
}

/// <summary>A cut: the clip it made, and the document state after its removal.</summary>
public sealed record ContentCut(ContentClip Clip, AnnotationState State);

/// <summary>
/// The content clipboard (B25 T-209): copy, cut and paste of a multi-item
/// selection. A paste is one undo step, and dragging all of it again before a
/// save moves the paste itself.
/// </summary>
public sealed partial class PdfDocumentFacade
{
    /// <summary>Copies every member of <paramref name="selection"/>; the document does not change.</summary>
    public async Task<OperationResult<ContentClip>> CopyContentSelectionAsync(string sessionId, ContentSelection selection)
    {
        const string operation = "copy_content_selection";
        var members = Members(selection);
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<ContentClip>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, selection.PageIndex));
            }
            var clip = await Task.Run(() => _core.CopyContentSelection(session.Document, members, cut: false)).ConfigureAwait(false);
            return OperationResult<ContentClip>.Success(new ContentClip(clip));
        }
        catch (PdfCoreException error) { return OperationResult<ContentClip>.Failure(MapTextBlockError(error, operation, sessionId, selection.PageIndex)); }
        catch (Exception error) { return OperationResult<ContentClip>.Failure(MapUnexpected(error, operation, sessionId, selection.PageIndex)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>Copies every member of <paramref name="selection"/> and removes them, one undo step.</summary>
    public async Task<OperationResult<ContentCut>> CutContentSelectionAsync(string sessionId, ContentSelection selection)
    {
        PdfCoreContentClip? clip = null;
        var members = Members(selection);
        var removed = await WriteContentBatchAsync(sessionId, selection.PageIndex, "cut_content_selection",
            document => clip = _core.CopyContentSelection(document, members, cut: true)).ConfigureAwait(false);
        return removed.IsSuccess && clip is not null
            ? OperationResult<ContentCut>.Success(new ContentCut(new ContentClip(clip), removed.Value!))
            : OperationResult<ContentCut>.Failure(removed.Error!);
    }

    /// <summary>
    /// Writes <paramref name="clip"/> onto page <paramref name="pageIndex"/>
    /// with its top-left corner at (<paramref name="left"/>, <paramref name="top"/>), PDF points.
    /// </summary>
    public Task<OperationResult<AnnotationState>> PasteContentAsync(string sessionId, uint pageIndex, ContentClip clip,
        double left, double top)
    {
        if (!double.IsFinite(left) || !double.IsFinite(top))
        {
            return Task.FromResult(OperationResult<AnnotationState>.Failure(CreateError("Paste coordinates must be finite.",
                PdfCoreError.UnsupportedOperation, "paste_content", sessionId, pageIndex)));
        }
        return WriteContentBatchAsync(sessionId, pageIndex, "paste_content",
            document => _core.PasteContent(document, pageIndex, clip.Source, left, top));
    }

    private static PdfCoreContentSelectionEdit.Remove Members(ContentSelection selection) =>
        new(selection.RunSources, selection.ImageSources, selection.PathSources);
}
