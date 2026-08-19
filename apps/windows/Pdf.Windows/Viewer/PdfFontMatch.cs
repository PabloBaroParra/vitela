namespace Pdf.Windows.Viewer;

/// <summary>
/// The local font that stands in for a PDF's own, while a text run is being
/// edited in place.
/// </summary>
/// <remarks>
/// A page names its fonts the way the file does — <c>Helvetica-Bold</c>,
/// <c>ABCDEF+Times New Roman,Italic</c> — and none of that is a Windows font
/// name. The editor draws over the page in a real font, so a mismatch is not
/// cosmetic: a face with different advance widths puts the reader's text
/// somewhere the document's would never be, however carefully the box is
/// positioned.
///
/// Only the three Standard-14 families are translated, because only they are
/// guaranteed to be *absent* from the machine while being present in the file:
/// Helvetica, Times and Courier are PostScript names for faces Windows ships
/// under other names. Anything else is asked for by its own name and left to
/// the font fallback, which is the honest answer — a document embedding a font
/// this machine happens to have installed gets it, and one embedding a font it
/// does not falls back to the same neutral face as before.
/// </remarks>
public static class PdfFontMatch
{
    /// <summary>The face used when the page does not say, or names something unknown.</summary>
    public const string Fallback = "Segoe UI";

    /// <summary>
    /// A Windows font family list — the matched face first, the fallback
    /// after — and the style the name declares.
    /// </summary>
    public static (string Families, bool Bold, bool Italic) ForBaseFont(string? baseFont)
    {
        var name = Strip(baseFont);
        if (name.Length == 0)
        {
            return (Fallback, false, false);
        }

        var lowered = name.ToLowerInvariant();
        var bold = lowered.Contains("bold", StringComparison.Ordinal);
        var italic = lowered.Contains("italic", StringComparison.Ordinal)
            || lowered.Contains("oblique", StringComparison.Ordinal);

        var family = lowered switch
        {
            var value when value.StartsWith("helvetica", StringComparison.Ordinal) || value.StartsWith("arial", StringComparison.Ordinal) => "Arial",
            var value when value.StartsWith("times", StringComparison.Ordinal) => "Times New Roman",
            var value when value.StartsWith("courier", StringComparison.Ordinal) => "Courier New",
            var value when value.StartsWith("symbol", StringComparison.Ordinal) => "Segoe UI Symbol",
            var value when value.StartsWith("zapfdingbats", StringComparison.Ordinal) => "Wingdings",
            _ => FamilyPart(name),
        };

        return (family == Fallback ? family : $"{family}, {Fallback}", bold, italic);
    }

    /// <summary>
    /// Drops the six-letter subset tag a font gets when only the glyphs a
    /// document uses are embedded — <c>ABCDEF+Times</c> is Times, and the tag
    /// is about what was embedded, not about which face to ask for.
    /// </summary>
    private static string Strip(string? baseFont)
    {
        var name = baseFont?.Trim() ?? string.Empty;
        if (name.Length > 7 && name[6] == '+')
        {
            name = name[7..];
        }

        return name;
    }

    /// <summary>
    /// The family without the style that follows it: PDF names spell style
    /// with a hyphen or a comma (<c>Times-BoldItalic</c>,
    /// <c>Arial,Bold</c>), which no font on this machine is called.
    /// </summary>
    private static string FamilyPart(string name)
    {
        var cut = name.IndexOfAny([',', '-']);
        var family = (cut < 0 ? name : name[..cut]).Trim();
        return family.Length == 0 ? Fallback : family;
    }
}
