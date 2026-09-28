namespace Pdf.Windows.Viewer;

/// <summary>What the status line says once an extracted PDF is on disk.</summary>
/// <remarks>
/// Mirrors the Linux shell's `write::extract::options::extract_summary`:
/// singular/plural, where the file went, and — only when the source carries
/// one — a note that its signature no longer verifies. An extraction writes
/// a *new* file and leaves the open document (and the one it came from)
/// untouched, so there is no irreversible choice for the reader to consent
/// to; the signature note is said afterwards rather than asked as a warning.
/// </remarks>
public static class ExtractPagesWording
{
    public static string Summary(int count, string destination, bool sourceIsSigned)
    {
        var pages = count == 1 ? "page" : "pages";
        var summary = $"Extracted {count} {pages} to {destination}.";
        if (sourceIsSigned)
        {
            summary += " The original document is signed, so the extracted PDF's signature no longer verifies.";
        }

        return summary;
    }
}
