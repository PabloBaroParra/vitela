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

    public IPdfCoreDocument OpenWithPasswordsFromBytes(byte[] bytes, string openPassword, string permissionsPassword)
    {
        try
        {
            return new GeneratedDocument(PdfFfiMethods.OpenWithPasswordsFromBytes(bytes, openPassword, permissionsPassword));
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

    public IReadOnlyList<PdfCoreFormField> ListFormFields(IPdfCoreDocument document) =>
        [.. ((GeneratedDocument)document).Handle.ListFormFields().Select(field => new PdfCoreFormField(
            field.Id,
            // A `PageId`, which the FFI mints as the page's index — the same
            // reading every other page-keyed call on this boundary makes.
            field.Page,
            field.Name,
            FieldKind(field.Kind),
            FieldValue(field.Value),
            new FormTextStyle(FormFontFromCore(field.Style.Font), field.Style.SizePt,
                new AnnotationColor(field.Style.Color.R, field.Style.Color.G, field.Style.Color.B))))];

    private static FormFont FormFontFromCore(FfiFontFamily font) => font switch
    {
        FfiFontFamily.Helvetica => FormFont.Helvetica,
        FfiFontFamily.TimesRoman => FormFont.TimesRoman,
        FfiFontFamily.Courier => FormFont.Courier,
        _ => throw new InvalidOperationException("Unsupported form font."),
    };

    private static FfiFontFamily FormFontToCore(FormFont font) => font switch
    {
        FormFont.Helvetica => FfiFontFamily.Helvetica,
        FormFont.TimesRoman => FfiFontFamily.TimesRoman,
        FormFont.Courier => FfiFontFamily.Courier,
        _ => throw new ArgumentOutOfRangeException(nameof(font)),
    };

    private static FormFieldKind FieldKind(FfiFormFieldKind kind) => kind switch
    {
        FfiFormFieldKind.Text text => new FormFieldKind.Text(text.Multiline, text.MaxLen),
        FfiFormFieldKind.Checkbox => new FormFieldKind.Checkbox(),
        FfiFormFieldKind.RadioGroup radio => new FormFieldKind.RadioGroup([.. radio.Options.Select(option => option.ExportValue)]),
        FfiFormFieldKind.Dropdown dropdown => new FormFieldKind.Dropdown(dropdown.Options, dropdown.Editable),
        // The core already folds a kind it does not model into a read-only
        // text field; a binding newer than this shell gets the same answer.
        _ => new FormFieldKind.Text(Multiline: false, MaxLength: 0),
    };

    private static FormFieldValue FieldValue(FfiFieldValue value) => value switch
    {
        FfiFieldValue.Text text => new FormFieldValue.Text(text.TextValue),
        FfiFieldValue.Checked check => new FormFieldValue.Checked(check.CheckedValue),
        FfiFieldValue.Choice choice => new FormFieldValue.Choice(choice.Option),
        _ => throw new InvalidOperationException("Unsupported form field value."),
    };

    private static FfiFieldValue FieldValue(FormFieldValue value) => value switch
    {
        FormFieldValue.Text text => new FfiFieldValue.Text(text.Value),
        FormFieldValue.Checked check => new FfiFieldValue.Checked(check.Value),
        FormFieldValue.Choice choice => new FfiFieldValue.Choice(choice.Option),
        _ => throw new InvalidOperationException("Unsupported form field value."),
    };

    public IReadOnlyList<PdfCoreAnnotation> Annotations(IPdfCoreDocument document)
    {
        try
        {
            return [.. ((GeneratedDocument)document).Handle.Annotations().Select(ConvertAnnotation)];
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public PdfCoreDocumentInfo ReadDocumentInfo(IPdfCoreDocument document)
    {
        var info = ((GeneratedDocument)document).Handle.ReadDocumentInfo();
        return new PdfCoreDocumentInfo(info.Title, info.Author, info.Subject, info.Keywords, info.Creator, info.Producer, info.CreationDate, info.ModDate);
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

    public PlacedRect PlaceRect(AnnotationRect rect, PagePlacement page)
    {
        var placed = PdfFfiMethods.PlaceRect(new FfiRect(rect.X, rect.Y, rect.Width, rect.Height), Placement(page));
        return new PlacedRect(placed.Left, placed.Top, placed.Width, placed.Height);
    }

    public PlacedPoint PlacePoint(AnnotationPoint point, PagePlacement page)
    {
        var placed = PdfFfiMethods.PlacePoint(new FfiPoint(point.X, point.Y), Placement(page));
        return new PlacedPoint(placed.X, placed.Y);
    }

    public AnnotationPoint PointToPdf(PlacedPoint point, PagePlacement page)
    {
        var pdf = PdfFfiMethods.PointToPdf(new FfiPoint(point.Left, point.Top), Placement(page));
        return new AnnotationPoint(pdf.X, pdf.Y);
    }

    public bool Undo(IPdfCoreDocument document) => PdfFfiMethods.Undo(((GeneratedDocument)document).Handle);
    public bool Redo(IPdfCoreDocument document) => PdfFfiMethods.Redo(((GeneratedDocument)document).Handle);
    public bool WillInvalidateSignatures(IPdfCoreDocument document)
    {
        try { return PdfFfiMethods.WillInvalidateSignatures(((GeneratedDocument)document).Handle, FfiSaveIntent.Default); }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool ProtectionWillInvalidateSignatures(IPdfCoreDocument document)
    {
        try { return PdfFfiMethods.ProtectionWillInvalidateSignatures(((GeneratedDocument)document).Handle); }
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

    public byte[] ProtectToBytes(IPdfCoreDocument document, string openPassword, string permissionsPassword, bool signaturesAcknowledged)
    {
        var acknowledgement = signaturesAcknowledged
            ? FfiSignatureAcknowledgement.ProceedAndInvalidate
            : FfiSignatureAcknowledgement.Unacknowledged;
        try { return PdfFfiMethods.ProtectToBytes(((GeneratedDocument)document).Handle, openPassword, permissionsPassword, acknowledgement); }
        catch (FfiException error) { throw Translate(error); }
    }

    public string? CompressionRefusal(IPdfCoreDocument document)
    {
        var refusal = PdfFfiMethods.CompressionRefusal(((GeneratedDocument)document).Handle, FfiSaveIntent.Default);
        return refusal is null ? null : RefusalSentence(refusal);
    }

    public bool CompressedSaveWillInvalidateSignatures(IPdfCoreDocument document)
    {
        try { return PdfFfiMethods.CompressedSaveWillInvalidateSignatures(((GeneratedDocument)document).Handle, FfiSaveIntent.Default); }
        catch (FfiException error) { throw Translate(error); }
    }

    public PdfCoreCompressedSave SaveCompressedToBytes(IPdfCoreDocument document, PdfCoreCompressPreset preset, bool signaturesAcknowledged)
    {
        var acknowledgement = signaturesAcknowledged
            ? FfiSignatureAcknowledgement.ProceedAndInvalidate
            : FfiSignatureAcknowledgement.Unacknowledged;
        var ffiPreset = preset switch
        {
            PdfCoreCompressPreset.Lossless => FfiCompressPreset.Lossless,
            PdfCoreCompressPreset.Balanced => FfiCompressPreset.Balanced,
            PdfCoreCompressPreset.Small => FfiCompressPreset.Small,
            _ => throw new ArgumentOutOfRangeException(nameof(preset)),
        };
        try
        {
            var saved = PdfFfiMethods.SaveCompressedToBytes(((GeneratedDocument)document).Handle, FfiSaveIntent.Default, acknowledgement, ffiPreset);
            var report = saved.Report;
            return new PdfCoreCompressedSave(
                saved.Bytes,
                report.BeforeBytes,
                report.AfterBytes,
                report.SavedBytes,
                report.Outcome == FfiCompressOutcome.Reduced,
                [.. report.Refusals.Select(RefusalSentence)]);
        }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool TextExtractionAllowed(IPdfCoreDocument document) =>
        ((GeneratedDocument)document).Handle.TextExtractionAllowed();

    public IReadOnlyList<uint> ParsePageSelection(string input, uint totalPages)
    {
        try { return PdfFfiMethods.ParsePageSelection(input, totalPages); }
        catch (FfiException error) { throw Translate(error); }
    }

    public string PageImageFileName(string documentName, uint pageIndex, uint totalPages, PdfCoreImageFormat format) =>
        PdfFfiMethods.PageImageFileName(documentName, pageIndex, totalPages, ImageFormat(format));

    public uint? FirstPageTooLargeToExport(IPdfCoreDocument document, IReadOnlyList<uint> pages, uint dpi) =>
        PdfFfiMethods.FirstPageTooLargeToExport(((GeneratedDocument)document).Handle, [.. pages], dpi);

    public byte[] ExportPageImage(IPdfCoreDocument document, uint pageIndex, uint dpi, PdfCoreImageFormat format)
    {
        try { return PdfFfiMethods.ExportPageImage(((GeneratedDocument)document).Handle, pageIndex, dpi, ImageFormat(format)); }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool FullRewriteAllowed(IPdfCoreDocument document) =>
        ((GeneratedDocument)document).Handle.FullRewriteAllowed();

    public byte[] ExtractPagesToPdf(IPdfCoreDocument document, IReadOnlyList<uint> pages)
    {
        try { return PdfFfiMethods.ExtractPagesToPdf(((GeneratedDocument)document).Handle, [.. pages]); }
        catch (FfiException error) { throw Translate(error); }
    }

    public bool ExtractSourceIsSigned(IPdfCoreDocument document) =>
        PdfFfiMethods.ExtractSourceIsSigned(((GeneratedDocument)document).Handle);

    public IReadOnlyList<SplitPart> PlanSplit(string cuts, uint totalPages, string documentName)
    {
        try { return [.. PdfFfiMethods.PlanSplit(cuts, totalPages, documentName)
            .Select(part => new SplitPart(part.First, part.Last, part.FileName))]; }
        catch (FfiException error) { throw Translate(error); }
    }

    private static FfiExportFormat ImageFormat(PdfCoreImageFormat format) => format switch
    {
        PdfCoreImageFormat.Png => FfiExportFormat.Png,
        PdfCoreImageFormat.Jpeg => FfiExportFormat.Jpeg,
        _ => throw new ArgumentOutOfRangeException(nameof(format)),
    };

    // The two modelled refusals cross the boundary without text, so their
    // wording is restated here verbatim from `pdf_compress::Refusal`'s Display
    // — the sentence the GTK shell shows — and `Other` carries the core's own.
    private static string RefusalSentence(FfiCompressRefusal refusal) => refusal switch
    {
        FfiCompressRefusal.EncryptedDocumentNotRewritable => "this document's password protection does not allow it to be rewritten, so it cannot be compressed",
        FfiCompressRefusal.SignaturesWouldBeInvalidated => "compressing rewrites the file and would stop its signature from verifying",
        FfiCompressRefusal.Other other => other.Detail,
        _ => "the compression was refused for a reason this build does not recognise",
    };

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
        PdfCoreEdit.SetDocumentInfo value => new FfiEditCommand.SetDocumentInfo(new FfiDocumentInfo(
            value.After.Title,
            value.After.Author,
            value.After.Subject,
            value.After.Keywords,
            value.After.Creator,
            value.After.Producer,
            (FfiPdfDate?)value.After.CreationDate,
            (FfiPdfDate?)value.After.ModDate)),
        PdfCoreEdit.ReplaceTextRun value =>
            new FfiEditCommand.ReplaceTextRunContent(ContentRun(value.Item), value.After),
        PdfCoreEdit.ReplaceTextRunWithInsertedFont value =>
            new FfiEditCommand.ReplaceTextRunWithInsertedFont(ContentRun(value.Item), value.After),
        PdfCoreEdit.SetFieldValue value => new FfiEditCommand.SetFieldValue(value.FieldId, FieldValue(value.Value)),
        PdfCoreEdit.RenameFormField value => new FfiEditCommand.RenameFormField(value.FieldId, value.Name),
        PdfCoreEdit.RestyleFormField value => new FfiEditCommand.RestyleFormField(value.FieldId,
            new FfiTextStyle(FormFontToCore(value.Style.Font), value.Style.SizePt,
                new FfiColor(value.Style.Color.R, value.Style.Color.G, value.Style.Color.B))),
        PdfCoreEdit.AddTextField value => new FfiEditCommand.AddTextField(value.PageIndex, Rect(value.Rect),
            new FfiTextStyle(FfiFontFamily.Helvetica, 12, new FfiColor(0, 0, 0)), false, null),
        PdfCoreEdit.AddCheckbox value => new FfiEditCommand.AddCheckbox(value.PageIndex, Rect(value.Rect),
            new FfiTextStyle(FfiFontFamily.Helvetica, 12, new FfiColor(0, 0, 0))),
        PdfCoreEdit.AddRadioGroup value => new FfiEditCommand.AddDefaultRadioGroup(value.PageIndex, Rect(value.Rect),
            new FfiTextStyle(FfiFontFamily.Helvetica, 12, new FfiColor(0, 0, 0))),
        PdfCoreEdit.AddDropdown value => new FfiEditCommand.AddDropdown(value.PageIndex, Rect(value.Rect),
            new FfiTextStyle(FfiFontFamily.Helvetica, 12, new FfiColor(0, 0, 0)), ["Option 1", "Option 2"], false),
        PdfCoreEdit.InsertBlankPage value => new FfiEditCommand.InsertBlankPage(value.Index, new FfiPageSize.A4(),
            value.Orientation == PageOrientation.Landscape ? FfiOrientation.Landscape : FfiOrientation.Portrait),
        PdfCoreEdit.RotatePage value => new FfiEditCommand.RotatePage(value.PageIndex, value.DeltaDegrees),
        PdfCoreEdit.RemovePage value => new FfiEditCommand.RemovePage(value.PageIndex),
        PdfCoreEdit.MovePages value => new FfiEditCommand.MovePages(value.From, value.Count, value.To),
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

    private static FfiPagePlacement Placement(PagePlacement page) => new(page.WidthPt, page.HeightPt, Rotation(page.Rotation), page.Scale);

    private static FfiPageRotation Rotation(PageRotation rotation) => rotation switch
    {
        PageRotation.None => FfiPageRotation.None,
        PageRotation.Clockwise90 => FfiPageRotation.Clockwise90,
        PageRotation.Clockwise180 => FfiPageRotation.Clockwise180,
        PageRotation.Clockwise270 => FfiPageRotation.Clockwise270,
        _ => throw new ArgumentOutOfRangeException(nameof(rotation)),
    };

    internal static PageRotation Rotation(FfiPageRotation rotation) => rotation switch
    {
        FfiPageRotation.None => PageRotation.None,
        FfiPageRotation.Clockwise90 => PageRotation.Clockwise90,
        FfiPageRotation.Clockwise180 => PageRotation.Clockwise180,
        FfiPageRotation.Clockwise270 => PageRotation.Clockwise270,
        _ => throw new ArgumentOutOfRangeException(nameof(rotation)),
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
            FfiException.FormFieldNotFound => PdfCoreError.FormFieldNotFound,
            FfiException.InvalidImage => PdfCoreError.InvalidImage,
            FfiException.InvalidSaveRequest => PdfCoreError.InvalidSaveRequest,
            FfiException.SignaturesWouldBeInvalidated => PdfCoreError.SignaturesWouldBeInvalidated,
            FfiException.UnsupportedOperation => PdfCoreError.UnsupportedOperation,
            FfiException.EncodingGap => PdfCoreError.EncodingGap,
            FfiException.InvalidPageSelection => PdfCoreError.InvalidPageSelection,
            FfiException.RenderFailed => PdfCoreError.RenderFailed,
            FfiException.Io => PdfCoreError.Io,
            _ => PdfCoreError.Internal
        };
        // Only what the reader typed crosses — the character a font cannot
        // show, or the sentence about a page range. Everything else in a typed
        // failure is diagnostic.
        var readerFacingDetail = error switch
        {
            FfiException.EncodingGap gap => gap.character,
            FfiException.InvalidPageSelection selection => selection.detail,
            _ => null,
        };
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
        }

        public DocumentHandle Handle { get; }
        public uint PageCount => Handle.PageCount();

        /// <summary>
        /// Asked of the core on every read, not cached at open: a page edit
        /// moves, removes or turns pages, and the core reports sizes in the
        /// document's current order.
        /// </summary>
        public IReadOnlyList<PdfCorePageDimensions> PageDimensions =>
            [.. Handle.PageDimensions().Select(page => new PdfCorePageDimensions(page.WidthPt, page.HeightPt, Rotation(page.Rotation)))];

        public void Dispose() => Handle.Dispose();
    }
}
