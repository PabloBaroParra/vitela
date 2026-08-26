using uniffi.pdf_ffi;

namespace Pdf.Windows.Facade;

internal sealed class GeneratedPdfCore : IPdfCore
{
    public IPdfCoreDocument OpenFromBytes(byte[] bytes, string? password)
    {
        try
        {
            return new GeneratedDocument(PdfFfiMethods.OpenFromBytes(bytes, password));
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    /// <remarks>
    /// <c>CreateDocumentWithBlankPage</c>, not <c>CreateBlankDocument</c>: the
    /// latter returns a <em>zero-page</em> document whose size/orientation are
    /// only the defaults for pages inserted afterwards, and this shell exposes
    /// no page insertion, so it left the reader on "This document has no
    /// pages." with every annotation tool disabled (T-063).
    /// </remarks>
    public IPdfCoreDocument CreateBlank()
    {
        try
        {
            return new GeneratedDocument(PdfFfiMethods.CreateDocumentWithBlankPage(new FfiPageSize.A4(), FfiOrientation.Portrait));
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    public PdfCoreBitmap RenderPage(IPdfCoreDocument document, uint pageIndex, uint dpi, bool invertContentColors)
    {
        try
        {
            var generated = ((GeneratedDocument)document).Handle;
            using var bitmap = PdfFfiMethods.RenderPage(generated, pageIndex, dpi, new FfiRenderOptions(invertContentColors));
            var width = bitmap.Width();
            var height = bitmap.Height();
            var rgba = PdfBitmapRows.TightlyPacked(bitmap.GetPixels(), width, height, bitmap.Stride());
            return new PdfCoreBitmap(width, height, width * 4, rgba);
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    public IReadOnlyList<PdfCoreBitmap> RenderPageTiles(IPdfCoreDocument document, uint pageIndex, uint dpi, IReadOnlyList<PageRegion> tiles, bool invertContentColors)
    {
        try
        {
            var generated = ((GeneratedDocument)document).Handle;
            var requested = tiles.Select(tile => new FfiRenderTile(tile.LeftPx, tile.TopPx, tile.WidthPx, tile.HeightPx)).ToArray();
            var bitmaps = PdfFfiMethods.RenderPageTiles(generated, pageIndex, dpi, requested, new FfiRenderOptions(invertContentColors));
            var copied = new List<PdfCoreBitmap>(bitmaps.Length);
            try
            {
                foreach (var bitmap in bitmaps)
                {
                    var width = bitmap.Width();
                    var height = bitmap.Height();
                    copied.Add(new PdfCoreBitmap(width, height, width * 4, PdfBitmapRows.TightlyPacked(bitmap.GetPixels(), width, height, bitmap.Stride())));
                }
            }
            finally
            {
                // Every handle in the batch owns a core-side registry entry, so
                // they are released even when a later tile's copy throws.
                foreach (var bitmap in bitmaps)
                {
                    bitmap.Dispose();
                }
            }

            return copied;
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    public IReadOnlyList<PdfCoreSearchHit> Search(IPdfCoreDocument document, string query)
    {
        try
        {
            var generated = ((GeneratedDocument)document).Handle;
            return [.. generated.Search(query).Select(result => new PdfCoreSearchHit(
                result.PageIndex,
                result.Text,
                [.. result.CharacterBounds.Select(bounds => new PdfCoreSearchRect(
                    bounds.XPt,
                    bounds.YPt,
                    bounds.WidthPt,
                    bounds.HeightPt))]))];
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    public IPdfCorePageCharacters PageCharacters(IPdfCoreDocument document, uint pageIndex)
    {
        try
        {
            var generated = ((GeneratedDocument)document).Handle;
            return new GeneratedPageCharacters(generated.PageCharacters(pageIndex));
        }
        catch (FfiException error)
        {
            throw Translate(error);
        }
    }

    public IReadOnlyList<PdfCoreAnnotation> Annotations(IPdfCoreDocument document)
    {
        try
        {
            return [.. ((GeneratedDocument)document).Handle.Annotations().Select(ConvertAnnotation)];
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public PdfCorePageContent ReadPageContent(IPdfCoreDocument document, uint pageIndex)
    {
        try
        {
            var content = ((GeneratedDocument)document).Handle.ReadPageContent(pageIndex);
            return new PdfCorePageContent(
                [.. content.TextRuns.Select(run => new PdfCoreContentTextRun(run.Id, run.Page, Rect(run.Bbox), run.ResourceFontName, FontKind(run.FontKind), run.Text))],
                [.. content.Images.Select(image => new PdfCoreContentImage(image.Id, image.Page, Rect(image.Bbox), image.ResourceXobjectName))]);
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public IReadOnlyDictionary<string, string> PageFontFamilies(IPdfCoreDocument document, uint pageIndex)
    {
        try
        {
            return ((GeneratedDocument)document).Handle.PageFontFamilies(pageIndex);
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public void RefreshPreview(IPdfCoreDocument document)
    {
        try { PdfFfiMethods.RefreshPreview(((GeneratedDocument)document).Handle); }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool AnnotationEditingAllowed(IPdfCoreDocument document) => ((GeneratedDocument)document).Handle.AnnotationEditingAllowed();
    public bool ContentEditingAllowed(IPdfCoreDocument document) => ((GeneratedDocument)document).Handle.ContentEditingAllowed();
    public bool CanUndo(IPdfCoreDocument document) => ((GeneratedDocument)document).Handle.CanUndo();
    public bool CanRedo(IPdfCoreDocument document) => ((GeneratedDocument)document).Handle.CanRedo();

    public void ApplyEdit(IPdfCoreDocument document, PdfCoreEdit edit)
    {
        try { PdfFfiMethods.ApplyEdit(((GeneratedDocument)document).Handle, ConvertEdit(edit)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public void InsertImageStamp(IPdfCoreDocument document, uint pageIndex, byte[] imageBytes, PdfCoreRect rect)
    {
        try { PdfFfiMethods.InsertImageStamp(((GeneratedDocument)document).Handle, pageIndex, imageBytes, Rect(rect)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public PdfCoreRect StampPlacement(byte[] imageBytes, double anchorX, double anchorY)
    {
        try { return Rect(PdfFfiMethods.StampPlacement(imageBytes, anchorX, anchorY)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool Undo(IPdfCoreDocument document) => PdfFfiMethods.Undo(((GeneratedDocument)document).Handle);
    public bool Redo(IPdfCoreDocument document) => PdfFfiMethods.Redo(((GeneratedDocument)document).Handle);
    public bool WillInvalidateSignatures(IPdfCoreDocument document)
    {
        try { return PdfFfiMethods.WillInvalidateSignatures(((GeneratedDocument)document).Handle, FfiSaveIntent.Default); }
        catch (FfiException error) { throw Translate(error); }
    }

    public byte[] SaveToBytes(IPdfCoreDocument document, bool signaturesAcknowledged)
    {
        var acknowledgement = signaturesAcknowledged
            ? FfiSignatureAcknowledgement.ProceedAndInvalidate
            : FfiSignatureAcknowledgement.Unacknowledged;
        try { return PdfFfiMethods.SaveToBytes(((GeneratedDocument)document).Handle, FfiSaveIntent.Default, acknowledgement); }
        catch (FfiException error) { throw Translate(error); }
    }

    private static PdfCoreAnnotation ConvertAnnotation(FfiAnnotation annotation) => annotation.Kind switch
    {
        FfiAnnotationKind.Highlight value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Highlight, Rect(value.Rect), Color(value.Color), []),
        FfiAnnotationKind.Underline value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Underline, Rect(value.Rect), Color(value.Color), []),
        FfiAnnotationKind.Strikeout value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Strikeout, Rect(value.Rect), Color(value.Color), []),
        FfiAnnotationKind.Ink value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Ink, null, Color(value.Color), [.. value.Points.Select(point => new PdfCorePoint(point.X, point.Y))]),
        FfiAnnotationKind.Shape value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Shape, Rect(value.Rect), Color(value.Color), []),
        FfiAnnotationKind.TextNote value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.TextNote, Rect(value.Rect), null, []),
        FfiAnnotationKind.Stamp value => new(annotation.Id, annotation.Page, PdfCoreAnnotationKind.Stamp, Rect(value.Rect), null, []),
        _ => throw new InvalidOperationException("Unsupported annotation kind."),
    };

    private static FfiEditCommand ConvertEdit(PdfCoreEdit edit) => edit switch
    {
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.Highlight } value => new FfiEditCommand.AddHighlight(value.PageIndex, Rect(value.Rect), Color(value.Color)),
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.Underline } value => new FfiEditCommand.AddUnderline(value.PageIndex, Rect(value.Rect), Color(value.Color)),
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.Strikeout } value => new FfiEditCommand.AddStrikeout(value.PageIndex, Rect(value.Rect), Color(value.Color)),
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.Shape } value => new FfiEditCommand.AddShape(value.PageIndex, Rect(value.Rect), Color(value.Color)),
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.Ink } value => new FfiEditCommand.AddInk(value.PageIndex, [.. (value.Points ?? []).Select(point => new FfiPoint(point.X, point.Y))], Color(value.Color)),
        PdfCoreEdit.Add { Kind: PdfCoreAnnotationKind.TextNote } value => new FfiEditCommand.AddTextNote(value.PageIndex, Rect(value.Rect), value.Contents ?? "Note"),
        PdfCoreEdit.Remove value => new FfiEditCommand.RemoveAnnotation(value.AnnotationId),
        PdfCoreEdit.Move value => new FfiEditCommand.MoveAnnotation(value.AnnotationId, value.Dx, value.Dy),
        PdfCoreEdit.Resize value => new FfiEditCommand.ResizeAnnotation(value.AnnotationId, Rect(value.Rect)),
        PdfCoreEdit.Restyle value => new FfiEditCommand.RestyleAnnotation(value.AnnotationId, Color(value.Color)),
        PdfCoreEdit.ReplaceTextRun value =>
            new FfiEditCommand.ReplaceTextRunContent(ContentRun(value.Item), value.After),
        PdfCoreEdit.ReplaceTextRunWithInsertedFont value =>
            new FfiEditCommand.ReplaceTextRunWithInsertedFont(ContentRun(value.Item), value.After),
        _ => throw new InvalidOperationException("Unsupported annotation edit."),
    };

    private static FfiContentTextRun ContentRun(PdfCoreContentTextRun run) =>
        new(run.Id, run.PageIndex, Rect(run.Bbox), run.ResourceFontName, FontKind(run.FontKind), run.Text);

    private static PdfCoreFontKind FontKind(FfiFontKind kind) => kind switch
    {
        FfiFontKind.Standard14 => PdfCoreFontKind.Standard14,
        FfiFontKind.EmbeddedSimple => PdfCoreFontKind.EmbeddedSimple,
        // Composite is also where an unrecognised future kind lands, which is
        // the conservative answer: the shell treats it as "cannot be retyped"
        // rather than opening an editor the core would refuse.
        _ => PdfCoreFontKind.EmbeddedComposite,
    };

    private static FfiFontKind FontKind(PdfCoreFontKind kind) => kind switch
    {
        PdfCoreFontKind.Standard14 => FfiFontKind.Standard14,
        PdfCoreFontKind.EmbeddedSimple => FfiFontKind.EmbeddedSimple,
        _ => FfiFontKind.EmbeddedComposite,
    };

    private static FfiRect Rect(PdfCoreRect rect) => new(rect.X, rect.Y, rect.Width, rect.Height);
    private static PdfCoreRect Rect(FfiRect rect) => new(rect.X, rect.Y, rect.Width, rect.Height);
    private static FfiColor Color(PdfCoreColor color) => new(color.R, color.G, color.B);
    private static PdfCoreColor Color(FfiColor color) => new(color.R, color.G, color.B);

    private static PdfCoreException Translate(FfiException error)
    {
        var category = error switch
        {
            FfiException.PasswordRequired => PdfCoreError.PasswordRequired,
            FfiException.WrongPassword => PdfCoreError.WrongPassword,
            FfiException.UnsupportedSecurityHandler => PdfCoreError.UnsupportedSecurityHandler,
            FfiException.DocumentNotFound => PdfCoreError.DocumentNotFound,
            FfiException.BitmapNotFound => PdfCoreError.BitmapNotFound,
            FfiException.PageIndexOutOfBounds => PdfCoreError.PageIndexOutOfBounds,
            FfiException.AnnotationNotFound => PdfCoreError.AnnotationNotFound,
            FfiException.InvalidImage => PdfCoreError.InvalidImage,
            FfiException.InvalidSaveRequest => PdfCoreError.InvalidSaveRequest,
            FfiException.SignaturesWouldBeInvalidated => PdfCoreError.SignaturesWouldBeInvalidated,
            FfiException.UnsupportedOperation => PdfCoreError.UnsupportedOperation,
            FfiException.EncodingGap => PdfCoreError.EncodingGap,
            FfiException.RenderFailed => PdfCoreError.RenderFailed,
            FfiException.Io => PdfCoreError.Io,
            _ => PdfCoreError.Internal
        };
        // The character is the only detail that crosses: everything else in a
        // typed failure is diagnostic, and the reader is the one who typed
        // this one.
        var readerFacingDetail = error is FfiException.EncodingGap gap ? gap.character : null;
        return new PdfCoreException(category, error.GetType().Name, readerFacingDetail);
    }

    private sealed class GeneratedPageCharacters(FfiPageCharacters handle) : IPdfCorePageCharacters
    {
        public uint? CaretAt(double xPt, double yPt) => handle.CaretAt((float)xPt, (float)yPt);
        public string TextIn(uint anchor, uint focus) => handle.TextIn(anchor, focus);
        public IReadOnlyList<PdfCoreSearchRect> RectsIn(uint anchor, uint focus) =>
            [.. handle.RectsIn(anchor, focus).Select(bounds => new PdfCoreSearchRect(bounds.XPt, bounds.YPt, bounds.WidthPt, bounds.HeightPt))];
        public void Dispose() => handle.Dispose();
    }

    private sealed class GeneratedDocument : IPdfCoreDocument
    {
        public GeneratedDocument(DocumentHandle handle)
        {
            Handle = handle;
            // The handle's render-side bytes are fixed for its lifetime
            // (render staleness model), so one FFI call covers all reads.
            PageDimensions = [.. handle.PageDimensions().Select(page => new PdfCorePageDimensions(page.WidthPt, page.HeightPt))];
        }

        public DocumentHandle Handle { get; }
        public uint PageCount => Handle.PageCount();
        public IReadOnlyList<PdfCorePageDimensions> PageDimensions { get; }

        public void Dispose() => Handle.Dispose();
    }
}
