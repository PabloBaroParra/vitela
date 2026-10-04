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
/// Decides what a dropped file is for, from its content signature.
///
/// The two outcomes are unrelated operations — a stamp edits the open
/// document, a PDF replaces it — so the choice is made once, up front, instead
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
    /// The first local file in the drop.
    ///
    /// Dragging several files at once is usually an accident — a whole folder
    /// selection swept in — and neither opening every PDF nor stamping every
    /// image is ever what was meant, so exactly one wins and the rest are
    /// ignored. Its content decides whether it is supported after the drag
    /// payload is released.
    /// </summary>
    public static T? FirstFile<T>(IEnumerable<T> items) where T : class
    {
        return items.FirstOrDefault();
    }
}
