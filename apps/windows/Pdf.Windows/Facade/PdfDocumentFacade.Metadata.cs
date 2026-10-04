namespace Pdf.Windows.Facade;

public sealed partial class PdfDocumentFacade
{
    /// <summary>Changes only the edited field against the effective current Info dictionary.</summary>
    public Task<OperationResult<DocumentInfo>> SetDocumentPropertyAsync(string sessionId, DocumentProperty property, string text) =>
        MutateDocumentInfoAsync(sessionId, current => property switch
        {
            DocumentProperty.Title => current with { Title = EmptyToNull(text) },
            DocumentProperty.Author => current with { Author = EmptyToNull(text) },
            DocumentProperty.Subject => current with { Subject = EmptyToNull(text) },
            DocumentProperty.Keywords => current with { Keywords = EmptyToNull(text) },
            DocumentProperty.Creator => current with { Creator = EmptyToNull(text) },
            DocumentProperty.Producer => current with { Producer = EmptyToNull(text) },
            _ => throw new ArgumentOutOfRangeException(nameof(property)),
        });

    /// <summary>Parses display text and preserves the effective date's offset inside the serialized edit boundary.</summary>
    public Task<OperationResult<DocumentInfo>> SetDocumentDateAsync(string sessionId, DocumentDateProperty property, string text) =>
        MutateDocumentInfoAsync(sessionId, current =>
        {
            var prior = (property == DocumentDateProperty.Created ? current.CreationDate : current.ModDate) as MetadataDate;
            var date = MetadataDateText.Parse(text, prior?.Offset ?? new());
            return property switch
            {
                DocumentDateProperty.Created => current with { CreationDate = date },
                DocumentDateProperty.Modified => current with { ModDate = date },
                _ => throw new ArgumentOutOfRangeException(nameof(property)),
            };
        });

    private async Task<OperationResult<DocumentInfo>> MutateDocumentInfoAsync(string sessionId, Func<PdfCoreDocumentInfo, PdfCoreDocumentInfo> mutate)
    {
        await _documentChangeGate.WaitAsync().ConfigureAwait(false);
        try
        {
            lock (_gate)
            {
                if (!TryGetCurrentSession(sessionId, out var session))
                    return OperationResult<DocumentInfo>.Failure(CreateError("The document is no longer available.", PdfCoreError.DocumentNotFound, "document_info", sessionId, null));
                if (!session.ContentEditingAllowed)
                    return OperationResult<DocumentInfo>.Failure(CreateError("This document does not permit metadata changes.", PdfCoreError.UnsupportedOperation, "document_info", sessionId, null));
                try
                {
                    var before = _core.ReadDocumentInfo(session.Document);
                    var after = mutate(before);
                    if (after != before)
                    {
                        _core.ApplyEdit(session.Document, new PdfCoreEdit.SetDocumentInfo(after));
                        session.EditRevision++;
                    }
                    return OperationResult<DocumentInfo>.Success(ToDocumentInfo(after));
                }
                catch (FormatException error)
                {
                    return OperationResult<DocumentInfo>.Failure(new UserSafeError($"Invalid date, reverted: {error.Message}", "metadata-date-validation"));
                }
                catch (PdfCoreException error)
                {
                    return OperationResult<DocumentInfo>.Failure(MapError(error, "document_info", sessionId, null));
                }
            }
        }
        finally { _documentChangeGate.Release(); }
    }
}
