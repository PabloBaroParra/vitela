using System.Globalization;
using System.Text;

namespace Pdf.Windows.Recovery;

/// <summary>
/// The sidecar beside each recovery entry's bytes: when they were written, what
/// the document was called, where it came from, and whether the bytes already
/// broke a signature.
/// </summary>
/// <remarks>
/// The same line format as the Linux shell's: <c>field TAB value</c> under a
/// versioned header. Fields this version does not know are skipped, so a later
/// version can add one without making older builds throw its entries away.
/// </remarks>
/// <param name="SavedAt">When the bytes were written, in Unix seconds.</param>
/// <param name="Name">The document's display name.</param>
/// <param name="Origin">The file it was opened from or last saved to; none for a new or sample document.</param>
/// <param name="BreaksSignatures">
/// Whether the bytes broke a signature the original file carries. A restored
/// document carries it into its session, so its Save still asks before writing
/// over the signed original.
/// </param>
public sealed record RecoveryMeta(long SavedAt, string Name, string? Origin, bool BreaksSignatures)
{
    /// <summary>First line of every sidecar. A different first line is a different format, discarded rather than guessed at.</summary>
    internal const string Header = "vitela-recovery 1";

    /// <summary>Written only when true, so a sidecar from before the field existed reads as false.</summary>
    private const string BreaksSignaturesField = "breaks-signatures";

    /// <summary>
    /// The name is percent-escaped and the origin stored as a <c>file:</c> URI:
    /// both are the user's text, and neither may break a line.
    /// </summary>
    public string Encode()
    {
        var text = new StringBuilder()
            .Append(Header).Append('\n')
            .Append("saved\t").Append(SavedAt.ToString(CultureInfo.InvariantCulture)).Append('\n')
            .Append("name\t").Append(Uri.EscapeDataString(Name)).Append('\n');
        // An empty or relative origin is no origin: it must never cost the write.
        if (Uri.TryCreate(Origin, UriKind.Absolute, out var origin) && origin.IsFile) text.Append("origin\t").Append(origin.AbsoluteUri).Append('\n');
        if (BreaksSignatures) text.Append(BreaksSignaturesField).Append("\t1\n");
        return text.ToString();
    }

    /// <summary>
    /// <c>null</c> only for a different format, or a sidecar missing the time or
    /// the name — the fields an entry cannot be offered without. Any other line
    /// that does not read is skipped: refusing the sidecar would delete the
    /// user's only copy of their work over it.
    /// </summary>
    public static RecoveryMeta? Parse(string text)
    {
        var lines = text.Split('\n').Select(line => line.TrimEnd('\r')).ToArray();
        if (lines.Length == 0 || lines[0] != Header) return null;
        long? savedAt = null;
        string? name = null;
        string? origin = null;
        var breaksSignatures = false;
        foreach (var line in lines.Skip(1))
        {
            var tab = line.IndexOf('\t');
            if (tab < 0) continue;
            var (field, value) = (line[..tab], line[(tab + 1)..]);
            switch (field)
            {
                case "saved":
                    savedAt = long.TryParse(value, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var seconds) ? seconds : null;
                    break;
                case "name":
                    name = Unescape(value);
                    break;
                case "origin":
                    origin = Uri.TryCreate(value, UriKind.Absolute, out var uri) && uri.IsFile ? uri.LocalPath : null;
                    break;
                // Fails closed: only an explicit "0" clears the flag. A value this
                // version does not know may be a later one's way of saying a
                // signature broke.
                case BreaksSignaturesField:
                    breaksSignatures = value != "0";
                    break;
            }
        }
        return savedAt is { } time && name is not null ? new RecoveryMeta(time, name, origin, breaksSignatures) : null;
    }

    private static string? Unescape(string value)
    {
        try { return Uri.UnescapeDataString(value); }
        catch (UriFormatException) { return null; }
    }
}
