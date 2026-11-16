namespace Pdf.Windows.Viewer;

/// <summary>The group an entry is listed under when the search is empty.</summary>
internal enum PaletteCategory { Mode, File, Edit, Annotate, Forms, Document, Pages, Sign }

/// <summary>
/// Every command the palette can run: a closed set, so the shell's dispatch
/// (<c>MainWindow.PaletteCommands.cs</c>) is an exhaustive switch.
/// </summary>
internal enum PaletteCommand
{
    ModeHome, ModeEdit, ModeAnnotate, ModeOrganize, ModeSign, ModeProtect,
    Open, New, Save, Print,
    Undo, Redo, Find,
    Highlight, Underline, Strikeout, Ink, Note, TextBox, Shape, Stamp,
    TextField, Checkbox, RadioGroup, Dropdown,
    ExportImages, Watermark, Compress,
    AddPdfs, Extract, Split,
    SignWithFile, SignWithToken, SignWithSystemCertificate, DrawSignature,
}

/// <summary>
/// What the palette's availability rules read: the same facts the buttons'
/// gates read (<c>UpdateAnnotationControls</c>, <c>RefreshSessionCommands</c>,
/// <c>UpdateFormToolbar</c>, <c>UpdateSigningControls</c>), minus the screen
/// (organizing) and busy parts: an entry takes the user to its button's
/// screen first, and the palette does not open while the shell is busy.
/// </summary>
/// <param name="AnnotationEditingAllowed">The annotation snapshot's permission; null until it arrives.</param>
/// <param name="ContentEditingAllowed">The session's content-change permission.</param>
/// <param name="FormStructureAllowed">The form snapshot's structure permission; null until it arrives.</param>
/// <param name="SigningLoaded">Whether the signing snapshot for this session has arrived.</param>
/// <param name="SigningRefusal">The core's signing refusal, when it has one.</param>
/// <param name="TextExtractionRefusal">The search's own refusal (<c>/P</c> bit 5), when the document has one.</param>
internal sealed record PaletteContext(
    bool HasDocument,
    int PageCount,
    bool? AnnotationEditingAllowed,
    bool ContentEditingAllowed,
    bool? FormStructureAllowed,
    bool SigningLoaded,
    string? SigningRefusal,
    bool CanUndo,
    bool CanRedo,
    string? TextExtractionRefusal)
{
    public static readonly PaletteContext NoDocument = new(false, 0, null, false, null, false, null, false, false, null);

    public bool HasPages => HasDocument && PageCount > 0;
}

/// <summary>
/// One palette command. Holds no behaviour (B27 decision 2): the shell runs
/// <see cref="Command"/> through the handler its button already calls.
/// </summary>
/// <param name="Id">Stable and unique; the session recents key on it. The GTK catalog's ids.</param>
/// <param name="Title">The English label, the one on the button the entry mirrors.</param>
/// <param name="Keywords">English search synonyms, space-separated (GTK <c>palette-*-keywords</c>).</param>
/// <param name="SpanishTitle">
/// The GTK shell's Spanish label. Search-only: the Windows UI is English,
/// but a Spanish speaker can still type the word they think in.
/// </param>
/// <param name="SpanishKeywords">The GTK shell's Spanish synonyms, search-only.</param>
internal sealed record PaletteEntry(
    string Id,
    PaletteCommand Command,
    PaletteCategory Category,
    string Title,
    ShellShortcut? Shortcut,
    string Keywords,
    string SpanishTitle,
    string SpanishKeywords)
{
    /// <summary>Null when the entry can run now; otherwise why not, shown under its dimmed title.</summary>
    public string? Refusal(PaletteContext context) => PaletteCatalog.Refusal(Command, context);

    /// <summary>
    /// The texts a query is matched against: each language's title with its
    /// synonyms, so a query may mix them ("unir pdf" finds "Añadir PDF").
    /// </summary>
    public IReadOnlyList<string> Haystacks() => [$"{Title} {Keywords}", $"{SpanishTitle} {SpanishKeywords}"];
}

/// <summary>A search result: the entry and, when it cannot run now, why.</summary>
internal sealed record PaletteResult(PaletteEntry Entry, string? Refusal);

/// <summary>
/// The command palette's catalog (port of the GTK shell's
/// <c>palette/entries</c>, B27 T-229/T-231): which commands exist, what they
/// are called and searched by, which shortcut they show and whether they can
/// run now. Refusal sentences reuse the wording the shell already shows for
/// the same gate wherever one exists.
///
/// Not ported: GTK's "Rotate all pages right/left" — the Windows shell has no
/// whole-document rotate handler on main to walk through.
/// </summary>
internal static class PaletteCatalog
{
    public const string AnnotationRefusal = "This document does not allow annotation edits.";
    public const string ContentRefusal = "This document does not permit content changes.";
    public const string FormRefusal = "This document does not permit creating form fields.";
    public const string ProtectRefusal = "This document does not permit changes to its protection.";
    public const string StillOpening = "This document is still opening.";
    public const string SplitNeedsTwoPages = "Split needs a document with at least two pages.";

    /// <summary>
    /// Why Organize cannot open on a document right now, or null when it can:
    /// the one gate every door into Organize asks — the rail, Home's tool and
    /// this palette (GTK <c>organize::open_refusal</c>, the content permission).
    /// </summary>
    public static string? OrganizeOpenRefusal(bool contentEditingAllowed) => contentEditingAllowed ? null : ContentRefusal;

    /// <summary>Every command, in catalog order: modes, file, edit, annotate, forms, document, pages, sign.</summary>
    public static readonly IReadOnlyList<PaletteEntry> Entries =
    [
        new("mode.home", PaletteCommand.ModeHome, PaletteCategory.Mode, "Home", null,
            "start welcome recent files", "Inicio", "inicio portada recientes archivos"),
        new("mode.edit", PaletteCommand.ModeEdit, PaletteCategory.Mode, "Edit", null,
            "change modify content text image retype", "Editar", "cambiar modificar contenido texto imagen"),
        new("mode.annotate", PaletteCommand.ModeAnnotate, PaletteCategory.Mode, "Annotate", null,
            "comment markup review notes", "Anotar", "comentar comentario revisar notas marcar"),
        new("mode.organize", PaletteCommand.ModeOrganize, PaletteCategory.Mode, "Organize", null,
            "pages reorder arrange delete move thumbnails", "Organizar", "páginas página ordenar reordenar borrar eliminar mover miniaturas"),
        new("mode.sign", PaletteCommand.ModeSign, PaletteCategory.Mode, "Sign", null,
            "signature certificate fill forms", "Firmar", "firma certificado rellenar formulario"),
        new("mode.protect", PaletteCommand.ModeProtect, PaletteCategory.Mode, "Protect", null,
            "password encrypt lock security permissions", "Proteger", "contraseña cifrar encriptar bloquear seguridad permisos"),

        new("file.open", PaletteCommand.Open, PaletteCategory.File, "Open file…", ShellShortcuts.Open,
            "load browse document", "Abrir archivo…", "cargar documento examinar"),
        new("file.new", PaletteCommand.New, PaletteCategory.File, "New blank PDF", ShellShortcuts.New,
            "create empty document", "Nuevo PDF en blanco", "crear vacío documento"),
        new("file.save", PaletteCommand.Save, PaletteCategory.File, "Save", ShellShortcuts.Save,
            "copy keep write download", "Guardar como", "guardar copia conservar descargar"),
        new("file.print", PaletteCommand.Print, PaletteCategory.File, "Print", ShellShortcuts.Print,
            "printer paper", "Imprimir", "impresora papel"),

        new("edit.undo", PaletteCommand.Undo, PaletteCategory.Edit, "Undo", ShellShortcuts.Undo,
            "revert back cancel", "Deshacer", "revertir volver atrás cancelar"),
        new("edit.redo", PaletteCommand.Redo, PaletteCategory.Edit, "Redo", ShellShortcuts.Redo,
            "repeat again", "Rehacer", "repetir"),
        new("edit.find", PaletteCommand.Find, PaletteCategory.Edit, "Find in document", ShellShortcuts.Find,
            "search look locate text", "Buscar en el documento", "buscar búsqueda localizar encontrar texto"),

        new("annotate.highlight", PaletteCommand.Highlight, PaletteCategory.Annotate, "Highlight", null,
            "marker mark color yellow", "Resaltado", "resaltar marcador marcar color amarillo"),
        new("annotate.underline", PaletteCommand.Underline, PaletteCategory.Annotate, "Underline", null,
            "line under mark", "Subrayado", "subrayar línea marcar"),
        new("annotate.strikeout", PaletteCommand.Strikeout, PaletteCategory.Annotate, "Strikeout", null,
            "strikethrough cross out line", "Tachado", "tachar línea"),
        new("annotate.ink", PaletteCommand.Ink, PaletteCategory.Annotate, "Ink", null,
            "draw pen pencil freehand", "Tinta", "dibujar lápiz bolígrafo mano alzada"),
        new("annotate.note", PaletteCommand.Note, PaletteCategory.Annotate, "Note", null,
            "comment sticky popup", "Nota", "comentario comentar adhesiva"),
        new("annotate.text-box", PaletteCommand.TextBox, PaletteCategory.Annotate, "Text box", null,
            "type write free text label", "Cuadro de texto", "escribir texto libre etiqueta"),
        new("annotate.shape", PaletteCommand.Shape, PaletteCategory.Annotate, "Shape", null,
            "rectangle square circle ellipse line arrow", "Forma", "rectángulo cuadrado círculo elipse línea flecha"),
        new("annotate.stamp", PaletteCommand.Stamp, PaletteCategory.Annotate, "Stamp", null,
            "approved seal image", "Sello", "aprobado timbre imagen"),

        new("forms.text", PaletteCommand.TextField, PaletteCategory.Forms, "Text field", null,
            "input form field", "Campo de texto", "formulario campo entrada"),
        new("forms.checkbox", PaletteCommand.Checkbox, PaletteCategory.Forms, "Checkbox", null,
            "tick check box form field", "Casilla de verificación", "marcar tilde formulario campo"),
        new("forms.radio-group", PaletteCommand.RadioGroup, PaletteCategory.Forms, "Radio group", null,
            "option button choice form field", "Grupo de opciones", "opción botón elegir formulario campo"),
        new("forms.dropdown", PaletteCommand.Dropdown, PaletteCategory.Forms, "Dropdown", null,
            "list select combo choice form field", "Lista desplegable", "lista seleccionar desplegable formulario campo opciones"),

        new("document.export-images", PaletteCommand.ExportImages, PaletteCategory.Document, "Export images", null,
            "png jpeg jpg picture convert", "Exportar imágenes", "exportar imagen imágenes png jpeg foto convertir"),
        new("document.watermark", PaletteCommand.Watermark, PaletteCategory.Document, "Add watermark", null,
            "stamp logo overlay background", "Añadir marca de agua", "marca agua sello logo fondo"),
        new("document.compress", PaletteCommand.Compress, PaletteCategory.Document, "Compress", null,
            "reduce shrink size smaller optimize", "Comprimir", "reducir tamaño comprimir achicar optimizar"),

        new("pages.add-pdfs", PaletteCommand.AddPdfs, PaletteCategory.Pages, "Add PDFs", null,
            "merge combine import insert", "Añadir PDF", "unir combinar fusionar importar insertar"),
        new("pages.extract", PaletteCommand.Extract, PaletteCategory.Pages, "Extract", null,
            "pages selection separate", "Extraer", "páginas página extraer separar selección"),
        new("pages.split", PaletteCommand.Split, PaletteCategory.Pages, "Split", null,
            "pages separate divide cut", "Dividir", "páginas página separar partir cortar"),

        new("sign.certificate-file", PaletteCommand.SignWithFile, PaletteCategory.Sign, "Choose signing certificate (.pfx)…", null,
            "signature digital pfx p12", "Elegir certificado de firma (.pfx)…", "firma firmar digital pfx p12"),
        new("sign.token", PaletteCommand.SignWithToken, PaletteCategory.Sign, "Use card or token…", null,
            "signature smart card digital", "Usar tarjeta o token…", "firma firmar tarjeta inteligente digital"),
        new("sign.system-certificate", PaletteCommand.SignWithSystemCertificate, PaletteCategory.Sign, "Use a certificate from this computer…", null,
            "signature digital installed", "Usar un certificado de este equipo…", "firma firmar digital instalado"),
        new("sign.draw", PaletteCommand.DrawSignature, PaletteCategory.Sign, "Draw signature…", null,
            "signature handwritten initials", "Dibujar firma…", "firma firmar manuscrita rúbrica iniciales"),
    ];

    /// <summary>
    /// The catalog ranked against <paramref name="query"/>, each with its
    /// refusal under <paramref name="context"/>. Refused entries stay listed
    /// (decision 3). An empty query keeps catalog order; putting the
    /// session's recents first is the overlay's job.
    /// </summary>
    public static IReadOnlyList<PaletteResult> Search(string query, PaletteContext context) =>
        PaletteMatcher.Rank(query, Entries, entry => entry.Haystacks())
            .Select(ranked => new PaletteResult(Entries[ranked.Index], Entries[ranked.Index].Refusal(context)))
            .ToList();

    /// <summary>
    /// Each command's gate, in the order its button's own gate asks it. Modes
    /// are navigation: with no document the rail picks one first, so they
    /// are never refused for its absence (decision 3's exception); Compress
    /// likewise, as a Home tool. With a document, Organize and Protect state
    /// their door's refusal (GTK <c>mode_refusal</c>: Organize asks
    /// <c>organize::open_refusal</c>, the content permission).
    /// </summary>
    public static string? Refusal(PaletteCommand command, PaletteContext context) => command switch
    {
        PaletteCommand.ModeHome or PaletteCommand.ModeEdit or PaletteCommand.ModeAnnotate
            or PaletteCommand.ModeSign => null,
        PaletteCommand.ModeOrganize => context.HasDocument ? OrganizeOpenRefusal(context.ContentEditingAllowed) : null,
        PaletteCommand.ModeProtect => context.HasDocument && !context.ContentEditingAllowed ? ProtectRefusal : null,
        PaletteCommand.Open or PaletteCommand.New or PaletteCommand.Compress => null,
        PaletteCommand.Save => Pages(context, "Open a PDF before saving."),
        PaletteCommand.Print => Pages(context, "Open a PDF before printing."),
        PaletteCommand.Find => Pages(context, "Open a PDF before searching.") ?? context.TextExtractionRefusal,
        PaletteCommand.Undo => context.CanUndo ? null : "Nothing to undo.",
        PaletteCommand.Redo => context.CanRedo ? null : "Nothing to redo.",
        PaletteCommand.Highlight or PaletteCommand.Underline or PaletteCommand.Strikeout or PaletteCommand.Ink
            or PaletteCommand.Note or PaletteCommand.TextBox or PaletteCommand.Shape or PaletteCommand.Stamp =>
            Pages(context, "Open a PDF before annotating.") ?? Annotation(context),
        PaletteCommand.TextField or PaletteCommand.Checkbox or PaletteCommand.RadioGroup or PaletteCommand.Dropdown =>
            Pages(context, "Open a PDF before adding form fields.") ?? context.FormStructureAllowed switch
            {
                null => StillOpening,
                false => FormRefusal,
                true => null,
            },
        PaletteCommand.ExportImages => Pages(context, "Open a PDF before exporting it."),
        // AddWatermarkButton follows ContentEditButton: the annotation snapshot
        // must have arrived, then the session's content permission.
        PaletteCommand.Watermark => Pages(context, "Open a PDF before adding a watermark.")
            ?? (context.AnnotationEditingAllowed is null ? StillOpening : context.ContentEditingAllowed ? null : ContentRefusal),
        // Opening Organize is all the shell asks before Add PDFs; the core's
        // import refusal is asked by the button's own handler.
        PaletteCommand.AddPdfs => Pages(context, "Open a PDF before organizing its pages."),
        PaletteCommand.Extract => Pages(context, "Open a PDF before extracting pages."),
        PaletteCommand.Split => Pages(context, "Open a PDF before splitting it.") ?? (context.PageCount > 1 ? null : SplitNeedsTwoPages),
        PaletteCommand.SignWithFile or PaletteCommand.SignWithToken or PaletteCommand.SignWithSystemCertificate =>
            Pages(context, "Open a PDF before signing.") ?? (context.SigningLoaded ? context.SigningRefusal : StillOpening),
        // The same gate as the Stamp tool, which places it.
        PaletteCommand.DrawSignature => Pages(context, "Open a PDF before signing.") ?? Annotation(context),
        _ => throw new ArgumentOutOfRangeException(nameof(command), command, null),
    };

    private static string? Pages(PaletteContext context, string noDocument) => context.HasPages ? null : noDocument;

    private static string? Annotation(PaletteContext context) => context.AnnotationEditingAllowed switch
    {
        null => StillOpening,
        false => AnnotationRefusal,
        true => null,
    };
}
