namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    public async Task<OperationResult<TextInsertionTarget>> PrepareTextInsertionAsync(string sessionId, uint pageIndex)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                    return OperationResult<TextInsertionTarget>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "prepare_text_insertion", sessionId, pageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<TextInsertionTarget>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, "prepare_text_insertion", sessionId, pageIndex));
                if (pageIndex >= session.Document.PageCount)
                    return OperationResult<TextInsertionTarget>.Failure(CreateError("The page is no longer available.", PdfCoreError.PageIndexOutOfBounds, "prepare_text_insertion", sessionId, pageIndex));
                return OperationResult<TextInsertionTarget>.Success(new(sessionId, session.EditRevision, pageIndex));
            }
        }
        catch (PdfCoreException error) { return OperationResult<TextInsertionTarget>.Failure(MapError(error, "prepare_text_insertion", sessionId, pageIndex)); }
        catch (Exception error) { return OperationResult<TextInsertionTarget>.Failure(MapUnexpected(error, "prepare_text_insertion", sessionId, pageIndex)); }
        finally { _documentChangeGate.Release(); }
    }

    /// <summary>Appends a single line as real page content using a new Helvetica resource.</summary>
    public async Task<OperationResult<AnnotationState>> InsertTextRunAsync(string sessionId, TextInsertionTarget target, string text, double x, double y, double sizePt)
    {
        const string operation = "insert_text";
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            SessionEntry session;
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out session))
                    return OperationResult<AnnotationState>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, operation, sessionId, target.PageIndex));
                if (target.SessionId != sessionId || target.Revision != session.EditRevision)
                    return OperationResult<AnnotationState>.Failure(CreateError("The document changed. Please reopen the dialog.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
                if (!_core.ContentEditingAllowed(session.Document))
                    return OperationResult<AnnotationState>.Failure(CreateError("This document does not permit content changes.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
                if (string.IsNullOrWhiteSpace(text) || text.IndexOfAny(['\r', '\n']) >= 0)
                    return OperationResult<AnnotationState>.Failure(CreateError("Enter a nonempty single line of text.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
                if (!double.IsFinite(x) || !double.IsFinite(y) || !double.IsFinite(sizePt) || sizePt < 1 || sizePt > 72)
                    return OperationResult<AnnotationState>.Failure(CreateError("Coordinates must be finite and text size must be between 1 and 72 pt.", PdfCoreError.UnsupportedOperation, operation, sessionId, target.PageIndex));
            }
            // Check every declared font, including unused ones. A fresh name also avoids
            // pending insertions whose resources are not visible in the base document.
            var fonts = await Task.Run(() => _core.PageFontFamilies(session.Document, target.PageIndex)).ConfigureAwait(false);
            string fontName;
            do { fontName = "VitelaText" + Guid.NewGuid().ToString("N"); } while (fonts.ContainsKey(fontName));
            lock (_gate)
            {
                _core.ApplyEdit(session.Document, new PdfCoreEdit.InsertTextRun(new(
                    0, target.PageIndex, new(x, y, 200, sizePt), fontName, PdfCoreFontKind.Standard14, text)));
                session.EditRevision++;
                session.HasRecordedPreviewEdit = true;
            }
            return await RefreshPreviewAsync(session, operation, target.PageIndex).ConfigureAwait(false);
        }
        catch (PdfCoreException error) { return OperationResult<AnnotationState>.Failure(MapError(error, operation, sessionId, target.PageIndex)); }
        catch (Exception error) { return OperationResult<AnnotationState>.Failure(MapUnexpected(error, operation, sessionId, target.PageIndex)); }
        finally { _documentChangeGate.Release(); }
    }
}
