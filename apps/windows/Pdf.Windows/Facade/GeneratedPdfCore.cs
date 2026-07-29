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

    public IPdfCoreDocument CreateBlank()
    {
        try
        {
            return new GeneratedDocument(PdfFfiMethods.CreateBlankDocument(new FfiPageSize.A4(), FfiOrientation.Portrait));
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
            FfiException.UnsupportedOperation => PdfCoreError.UnsupportedOperation,
            FfiException.RenderFailed => PdfCoreError.RenderFailed,
            FfiException.Io => PdfCoreError.Io,
            _ => PdfCoreError.Internal
        };
        return new PdfCoreException(category, error.GetType().Name);
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
