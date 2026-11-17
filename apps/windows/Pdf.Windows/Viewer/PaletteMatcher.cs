using System.Text;

namespace Pdf.Windows.Viewer;

/// <summary>
/// Fuzzy matching for the command palette: pure, no XAML. A port of the GTK
/// shell's <c>palette/matcher.rs</c> (B27 decision 4), so a query ranks the
/// same commands the same way on both shells.
///
/// Matching is case- and accent-insensitive through <see cref="Normalize"/>.
/// The fold table is deliberately small and explicit — no
/// <c>string.Normalize(FormD)</c>: it covers the lowercase letters of Latin-1
/// Supplement and Latin Extended-A (Spanish, Portuguese, French and German,
/// plus ß→"ss", æ→"ae", œ→"oe"). Anything outside it passes through
/// lowercased but otherwise unchanged, so it still matches itself exactly.
/// </summary>
internal static class PaletteMatcher
{
    /// <summary>Points for every matched query character.</summary>
    internal const int Match = 1;
    /// <summary>Extra points when the matched character starts a word.</summary>
    internal const int WordStart = 8;
    /// <summary>Extra points when the matched character directly follows the previous one.</summary>
    internal const int Contiguous = 4;
    /// <summary>Extra points when a term's first character is the candidate's first.</summary>
    internal const int Prefix = 12;

    /// <summary>
    /// The whole-run bonus: a term that matches as a whole word or a word
    /// prefix earns <c>RunPerChar * len + RunBase</c> on top of its
    /// per-character points, which always beats any scattered alignment of
    /// the same term (the derivation is in <c>matcher.rs</c>).
    /// </summary>
    private const int RunPerChar = 8;
    private const int RunBase = 5;

    /// <summary>Lowercases <paramref name="text"/> and strips diacritics, so "Página", "PÁGINA" and "pagina" all become "pagina".</summary>
    internal static string Normalize(string text)
    {
        var output = new StringBuilder(text.Length);
        foreach (var c in text)
        {
            var lower = char.ToLowerInvariant(c);
            if (Fold(lower) is { } folded) output.Append(folded);
            else output.Append(lower);
        }
        return output.ToString();
    }

    /// <summary>The diacritic fold table; input is already lowercased.</summary>
    private static string? Fold(char c) => c switch
    {
        'à' or 'á' or 'â' or 'ã' or 'ä' or 'å' or 'ā' or 'ă' or 'ą' => "a",
        'æ' => "ae",
        'ç' or 'ć' or 'ĉ' or 'ċ' or 'č' => "c",
        'ď' or 'đ' => "d",
        'è' or 'é' or 'ê' or 'ë' or 'ē' or 'ĕ' or 'ė' or 'ę' or 'ě' => "e",
        'ĝ' or 'ğ' or 'ġ' or 'ģ' => "g",
        'ĥ' or 'ħ' => "h",
        'ì' or 'í' or 'î' or 'ï' or 'ĩ' or 'ī' or 'ĭ' or 'į' or 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' or 'ļ' or 'ľ' or 'ŀ' or 'ł' => "l",
        'ñ' or 'ń' or 'ņ' or 'ň' => "n",
        'ò' or 'ó' or 'ô' or 'õ' or 'ö' or 'ø' or 'ō' or 'ŏ' or 'ő' => "o",
        'œ' => "oe",
        'ŕ' or 'ŗ' or 'ř' => "r",
        'ś' or 'ŝ' or 'ş' or 'š' => "s",
        'ß' => "ss",
        'ţ' or 'ť' or 'ŧ' => "t",
        'ù' or 'ú' or 'û' or 'ü' or 'ũ' or 'ū' or 'ŭ' or 'ů' or 'ű' or 'ų' => "u",
        'ŵ' => "w",
        'ý' or 'ÿ' or 'ŷ' => "y",
        'ź' or 'ż' or 'ž' => "z",
        _ => null,
    };

    /// <summary>
    /// Scores <paramref name="candidate"/> against <paramref name="query"/>;
    /// null when it does not match. Every whitespace-separated term must
    /// appear in the candidate as a subsequence after normalizing; terms
    /// match independently, in any order. A query with no terms matches
    /// everything with 0, so an empty search keeps the catalog order.
    /// </summary>
    internal static int? Score(string query, string candidate) => ScoreTerms(Terms(query), candidate);

    /// <summary>
    /// Ranks <paramref name="items"/> against <paramref name="query"/>: each
    /// item scores the best of its haystacks. Returns the matching items'
    /// (index, score), best first; equal scores keep the catalog order.
    /// </summary>
    internal static IReadOnlyList<(int Index, int Score)> Rank<T>(string query, IReadOnlyList<T> items, Func<T, IReadOnlyList<string>> haystacks)
    {
        var terms = Terms(query);
        var ranked = new List<(int Index, int Score)>();
        for (var index = 0; index < items.Count; index++)
        {
            int? best = null;
            foreach (var haystack in haystacks(items[index]))
                if (ScoreTerms(terms, haystack) is { } score && (best is null || score > best)) best = score;
            if (best is { } value) ranked.Add((index, value));
        }
        // OrderBy is stable: equal scores keep the catalog order.
        return ranked.OrderByDescending(r => r.Score).ToList();
    }

    /// <summary>The bonus for a whole-word or word-prefix run of <paramref name="length"/> characters.</summary>
    internal static int RunBonus(int length) => RunPerChar * length + RunBase;

    private static string[] Terms(string query) =>
        Normalize(query).Split((char[]?)null, StringSplitOptions.RemoveEmptyEntries);

    private static int? ScoreTerms(string[] terms, string candidate)
    {
        var normalized = Normalize(candidate);
        var total = 0;
        foreach (var term in terms)
        {
            if (ScoreTerm(term, normalized) is not { } score) return null;
            total += score;
        }
        return total;
    }

    /// <summary>
    /// Best score of one term: every position of the term's first character
    /// is tried as a start, and each later character is placed by
    /// <see cref="NextPosition"/>. A whole-word or word-prefix run also earns
    /// <see cref="RunBonus"/>.
    /// </summary>
    private static int? ScoreTerm(string term, string candidate)
    {
        if (term.Length == 0) return null;
        int? best = null;
        for (var start = 0; start < candidate.Length; start++)
        {
            if (candidate[start] != term[0]) continue;
            var total = CharScore(candidate, start, null);
            var wholeRun = StartsWord(candidate, start);
            var previous = start;
            var placed = true;
            for (var index = 1; index < term.Length; index++)
            {
                if (NextPosition(candidate, previous, term[index], term.AsSpan(index + 1)) is not { } at)
                {
                    placed = false;
                    break;
                }
                total += CharScore(candidate, at, previous);
                wholeRun &= at == previous + 1;
                previous = at;
            }
            if (!placed) continue;
            if (wholeRun) total += RunBonus(term.Length);
            if (best is null || total > best) best = total;
        }
        return best;
    }

    /// <summary>
    /// Where <paramref name="wanted"/> goes after the character matched at
    /// <paramref name="previous"/>: directly after it when possible, so a
    /// contiguous run is never broken; else the first later word start that
    /// still leaves room for <paramref name="remaining"/>; else the earliest.
    /// </summary>
    private static int? NextPosition(string candidate, int previous, char wanted, ReadOnlySpan<char> remaining)
    {
        var after = previous + 1;
        if (after >= candidate.Length) return null;
        var found = candidate.IndexOf(wanted, after);
        if (found < 0) return null;
        if (found == after) return found;
        for (var at = found; at < candidate.Length; at++)
            if (candidate[at] == wanted && StartsWord(candidate, at) && IsSubsequence(remaining, candidate.AsSpan(at + 1)))
                return at;
        return found;
    }

    private static bool IsSubsequence(ReadOnlySpan<char> needle, ReadOnlySpan<char> haystack)
    {
        var position = 0;
        foreach (var c in needle)
        {
            while (position < haystack.Length && haystack[position] != c) position++;
            if (position == haystack.Length) return false;
            position++;
        }
        return true;
    }

    /// <summary>Whether the character at <paramref name="at"/> begins a word: the first character, or one after whitespace or - _ / ( ) . , :</summary>
    private static bool StartsWord(string candidate, int at)
    {
        if (at == 0) return true;
        var before = candidate[at - 1];
        return char.IsWhiteSpace(before) || before is '-' or '_' or '/' or '(' or ')' or '.' or ',' or ':';
    }

    private static int CharScore(string candidate, int at, int? previous)
    {
        var points = Match;
        if (StartsWord(candidate, at)) points += WordStart;
        if (at == 0 && previous is null) points += Prefix;
        if (previous is { } p && p + 1 == at) points += Contiguous;
        return points;
    }
}
