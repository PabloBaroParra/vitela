namespace Pdf.Windows.Viewer;

/// <summary>What the shell should do with a file the user dropped.</summary>
public enum DroppedFileKind
{
    /// <summary>Nothing this shell can open or place.</summary>
    Unsupported,
    /// <summary>An image to place on the page it landed on.</summary>
    ImageStamp,
    /// <summary>A PDF to open in a document tab of its own.</summary>
    Document,
}

/// <summary>
/// Decides what a dropped file is for, from its content signature.
///
/// The two outcomes are unrelated operations — a stamp edits the open
/// document, a PDF opens in a tab — so the choice is made once, up front, instead
/// of being left to whichever drop handler happens to run first. Content is
/// not decoded here: opening still goes through the core's own parse and
/// stamping through its builder, both of which report a real error for corrupt
/// content that has an otherwise supported signature.
/// </summary>
public static class FileDropRouting
{
    public static DroppedFileKind Classify(ReadOnlySpan<byte> bytes)
    {
        if (bytes.StartsWith("%PDF-"u8)) return DroppedFileKind.Document;
        return ImageStampInput.HasSupportedSignature(bytes) ? DroppedFileKind.ImageStamp : DroppedFileKind.Unsupported;
    }

    /// <summary>
    /// The first local file in the drop: the one a drop without any PDF acts
    /// on. Stamping every dropped image is never what was meant, so exactly
    /// one wins and the rest are ignored.
    /// </summary>
    public static T? FirstFile<T>(IEnumerable<T> items) where T : class
    {
        return items.FirstOrDefault();
    }

    /// <summary>
    /// What a drop of already-classified files comes to. Every PDF in it opens,
    /// each in its own document tab, in drop order — the files that are not
    /// PDFs are ignored then. A drop with no PDF acts on its first file, as a
    /// single-file drop always has.
    /// </summary>
    public static DropPlan<T> Plan<T>(IReadOnlyList<(T Item, DroppedFileKind Kind)> files) where T : class
    {
        var documents = files.Where(file => file.Kind == DroppedFileKind.Document).Select(file => file.Item).ToList();
        return documents.Count > 0
            ? new DropPlan<T>(documents, null)
            : new DropPlan<T>([], FirstFile(files.Select(file => file.Item)));
    }
}

/// <summary>The PDFs a drop opens, or else the one file it acts on.</summary>
public sealed record DropPlan<T>(IReadOnlyList<T> Documents, T? Single) where T : class;
