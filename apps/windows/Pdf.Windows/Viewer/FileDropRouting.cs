namespace Pdf.Windows.Viewer;

/// <summary>What the shell should do with a file the user dropped.</summary>
public enum DroppedFileKind
{
    /// <summary>Nothing this shell can open or place.</summary>
    Unsupported,
    /// <summary>An image to place on the page it landed on.</summary>
    ImageStamp,
    /// <summary>A PDF to open, replacing whatever is on screen.</summary>
    Document,
}

/// <summary>
/// Decides what a dropped file is for, from its name alone.
///
/// The two outcomes are unrelated operations — a stamp edits the open
/// document, a PDF replaces it — so the choice is made once, up front, instead
/// of being left to whichever drop handler happens to run first. Content is
/// not sniffed here: opening still goes through the core's own parse and
/// stamping through <see cref="ImageStampInput.HasSupportedSignature"/>, both
/// of which report a real error for a file that lied about its extension.
/// </summary>
public static class FileDropRouting
{
    public static DroppedFileKind Classify(string? path)
    {
        if (string.IsNullOrWhiteSpace(path)) return DroppedFileKind.Unsupported;
        if (Path.GetExtension(path).Equals(".pdf", StringComparison.OrdinalIgnoreCase)) return DroppedFileKind.Document;
        return ImageStampInput.HasSupportedFileExtension(path) ? DroppedFileKind.ImageStamp : DroppedFileKind.Unsupported;
    }

    /// <summary>
    /// The first file in the drop that the shell knows what to do with.
    ///
    /// Dragging several files at once is usually an accident — a whole folder
    /// selection swept in — and neither opening every PDF nor stamping every
    /// image is ever what was meant, so exactly one wins and the rest are
    /// ignored. Unsupported entries are skipped rather than failing the drop,
    /// so a PDF dragged alongside a README still opens.
    /// </summary>
    public static (T Item, DroppedFileKind Kind)? FirstActionable<T>(IEnumerable<T> items, Func<T, string?> path)
    {
        foreach (var item in items)
        {
            var kind = Classify(path(item));
            if (kind != DroppedFileKind.Unsupported) return (item, kind);
        }
        return null;
    }
}
