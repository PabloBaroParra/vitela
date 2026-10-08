namespace Pdf.Windows.Viewer;

/// <summary>
/// The name the shared copy carries. The receiving device shows it, and the
/// display name is untrusted text — a title, not a path — so anything Windows
/// would read as a separator or refuse in a file name is replaced.
/// </summary>
public static class ShareFileName
{
    private const string Fallback = "document";

    public static string For(string displayName)
    {
        var invalid = Path.GetInvalidFileNameChars();
        var name = new string(displayName.Trim().Select(c => invalid.Contains(c) ? '_' : c).ToArray()).Trim();
        if (name.Length == 0) name = Fallback;
        return name.EndsWith(".pdf", StringComparison.OrdinalIgnoreCase) ? name : name + ".pdf";
    }
}
