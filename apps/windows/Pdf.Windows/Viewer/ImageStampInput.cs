namespace Pdf.Windows.Viewer;

/// <summary>
/// Guards stamp work against the document changing underneath it, and routes
/// only the image encodings the core's stamp builder accepts.
/// </summary>
public static class ImageStampInput
{
    private static readonly byte[] PngSignature = [137, 80, 78, 71, 13, 10, 26, 10];

    public static bool HasSupportedFileExtension(string path) =>
        Path.GetExtension(path).Equals(".png", StringComparison.OrdinalIgnoreCase)
        || Path.GetExtension(path).Equals(".jpg", StringComparison.OrdinalIgnoreCase)
        || Path.GetExtension(path).Equals(".jpeg", StringComparison.OrdinalIgnoreCase);

    /// <summary>
    /// Checks only the encoding signature. The core performs the authoritative
    /// decode so corrupt-but-recognizable images get its typed error feedback.
    /// </summary>
    public static bool HasSupportedSignature(ReadOnlySpan<byte> bytes) =>
        bytes.StartsWith(PngSignature) || (bytes.Length >= 3 && bytes[0] == 0xff && bytes[1] == 0xd8 && bytes[2] == 0xff);

    /// <summary>
    /// Whether the session a stamp operation started against is still the one
    /// on screen. Inserting a stamp decodes a preview and crosses several
    /// awaits, any of which the reader can spend opening a different document,
    /// and neither the status line nor the preview cache belongs to that new
    /// document.
    /// </summary>
    public static bool SessionMatches(string capturedSessionId, string? currentSessionId) =>
        string.Equals(capturedSessionId, currentSessionId, StringComparison.Ordinal);
}
