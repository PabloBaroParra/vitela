//! Fuzzy matching for the command palette (B27 decision 4): pure, no GTK.
//!
//! Matching is case- and accent-insensitive through [`normalize`]. The fold
//! table is deliberately small and explicit — no `unicode-normalization`
//! crate: it covers the lowercase letters of Latin-1 Supplement and Latin
//! Extended-A (enough for Spanish, Portuguese, French and German, plus
//! `ß`→"ss", `æ`→"ae", `œ`→"oe"). Anything outside it (Greek, Cyrillic, CJK,
//! combining marks from decomposed input) passes through lowercased but
//! otherwise unchanged, so it still matches itself exactly.

/// Lowercases `text` and strips diacritics with the module's fold table, so
/// "Página", "PÁGINA" and "pagina" all become "pagina".
pub(crate) fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match fold(c) {
            Some(folded) => out.push_str(folded),
            None => out.push(c),
        }
    }
    out
}

/// The diacritic fold table: lowercase Latin-1 Supplement / Latin Extended-A
/// letter -> its unaccented ASCII spelling. Input is already lowercased.
fn fold(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' => "t",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// Points for every matched query character.
const MATCH: u32 = 1;
/// Extra points when the matched character starts a word.
const WORD_START: u32 = 8;
/// Extra points when the matched character directly follows the previous one.
const CONTIGUOUS: u32 = 4;
/// Extra points when a term's first character is the candidate's first.
const PREFIX: u32 = 12;

/// The whole-run bonus, per character of the term and once per term: a term
/// that matches as a whole word or a word prefix (it starts at a word start
/// and every character follows the previous one) earns
/// `RUN_PER_CHAR * len + RUN_BASE` on top of its per-character points.
///
/// Derived so such a run always beats any other alignment of the same term.
/// For a term of `n` characters, the best other alignment scores at most
/// `(MATCH + WORD_START + PREFIX) + (n - 1) * (MATCH + WORD_START + CONTIGUOUS)`
/// = `21 + 13(n - 1)` = `13n + 8` (a later character can be both a word start
/// and contiguous only when the term holds a separator, but the bound allows
/// it). The run scores at least `(MATCH + WORD_START) + (n - 1) * (MATCH +
/// CONTIGUOUS)` = `5n + 4` before the bonus, so the bonus must exceed
/// `8n + 4`: `8n + 5`. Without it "pdf" scored "Campo de texto formulario"
/// (p-d-f, d and f at word starts) the same 19 as "Añadir PDF", and the tie
/// went to catalog order.
const RUN_PER_CHAR: u32 = 8;
const RUN_BASE: u32 = 5;

/// Scores `candidate` against `query`; `None` when it does not match.
///
/// The query is split on whitespace into terms, and **every** term must
/// appear in the candidate as a subsequence (in order, gaps allowed) after
/// [`normalize`]. Terms are matched independently, in any order and possibly
/// overlapping, so "rotar pagina" and "pagina rotar" both match "Rotar
/// página". The score is the sum of the terms' scores; higher is better.
///
/// A query with no terms (empty or whitespace only) matches everything with
/// `Some(0)`, so an empty search keeps the catalog order.
///
/// Test-only: the catalog searches through [`rank`], which applies these
/// same rules to every haystack of every entry. This single-candidate form
/// is what the scoring tests pin.
#[cfg(test)]
pub(crate) fn score(query: &str, candidate: &str) -> Option<u32> {
    score_terms(&terms(query), candidate)
}

/// Ranks `items` against `query`: each item scores the best of its
/// `haystacks` (e.g. the label in the active language, the English label and
/// the keywords). Returns `(index, score)` for the items that match, best
/// first; equal scores keep the original catalog order.
pub(crate) fn rank<T>(
    query: &str,
    items: &[T],
    haystacks: impl Fn(&T) -> Vec<String>,
) -> Vec<(usize, u32)> {
    let terms = terms(query);
    let mut ranked: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            haystacks(item)
                .iter()
                .filter_map(|haystack| score_terms(&terms, haystack))
                .max()
                .map(|best| (index, best))
        })
        .collect();
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    ranked
}

fn terms(query: &str) -> Vec<Vec<char>> {
    normalize(query)
        .split_whitespace()
        .map(|term| term.chars().collect())
        .collect()
}

fn score_terms(terms: &[Vec<char>], candidate: &str) -> Option<u32> {
    let candidate: Vec<char> = normalize(candidate).chars().collect();
    terms.iter().try_fold(0u32, |total, term| {
        score_term(term, &candidate).map(|s| total.saturating_add(s))
    })
}

/// Best score of one term: every position where the term's first character
/// occurs is tried as a starting point, and each later character is placed
/// by [`next_position`]. Not an exhaustive alignment search, but it finds the
/// word-start or prefix alignment a user typing a label expects — "ep" puts
/// its `p` at "pages" in "Export pages", not inside "Export".
/// An alignment that is one whole-word or word-prefix run also earns
/// [`run_bonus`].
fn score_term(term: &[char], candidate: &[char]) -> Option<u32> {
    let (&first, rest) = term.split_first()?;
    candidate
        .iter()
        .enumerate()
        .filter(|&(_, &c)| c == first)
        .filter_map(|(start, _)| {
            let mut total = char_score(candidate, start, None);
            let mut whole_run = starts_word(candidate, start);
            let mut previous = start;
            for (index, &wanted) in rest.iter().enumerate() {
                let at = next_position(candidate, previous, wanted, &rest[index + 1..])?;
                total += char_score(candidate, at, Some(previous));
                whole_run &= at == previous + 1;
                previous = at;
            }
            if whole_run {
                total += run_bonus(term.len());
            }
            Some(total)
        })
        .max()
}

/// Where `wanted` goes after the character matched at `previous`, with
/// `remaining` still to place after it. In order of preference:
///
/// 1. directly after `previous`, so a contiguous run ("rot" in "Rotate")
///    is never broken up to chase a word start;
/// 2. the first later occurrence that starts a word, provided `remaining`
///    can still be matched after it;
/// 3. the earliest occurrence.
///
/// `None` when `wanted` does not occur after `previous` at all.
fn next_position(
    candidate: &[char],
    previous: usize,
    wanted: char,
    remaining: &[char],
) -> Option<usize> {
    let after = previous + 1;
    let earliest = after + candidate[after..].iter().position(|&c| c == wanted)?;
    if earliest == after {
        return Some(earliest);
    }
    let word_start = (earliest..candidate.len()).find(|&at| {
        candidate[at] == wanted
            && starts_word(candidate, at)
            && is_subsequence(remaining, &candidate[at + 1..])
    });
    Some(word_start.unwrap_or(earliest))
}

/// [`RUN_PER_CHAR`]/[`RUN_BASE`]'s bonus for a whole-word or word-prefix run
/// of `len` characters.
fn run_bonus(len: usize) -> u32 {
    u32::try_from(len)
        .unwrap_or(u32::MAX)
        .saturating_mul(RUN_PER_CHAR)
        .saturating_add(RUN_BASE)
}

fn is_subsequence(needle: &[char], haystack: &[char]) -> bool {
    let mut haystack = haystack.iter();
    needle.iter().all(|c| haystack.any(|h| h == c))
}

/// Whether the character at `at` begins a word: the candidate's first
/// character, or one after whitespace or `-`, `_`, `/`, `(`, `)`, `.`, `,`,
/// `:`.
fn starts_word(candidate: &[char], at: usize) -> bool {
    at == 0 || {
        let before = candidate[at - 1];
        before.is_whitespace() || matches!(before, '-' | '_' | '/' | '(' | ')' | '.' | ',' | ':')
    }
}

fn char_score(candidate: &[char], at: usize, previous: Option<usize>) -> u32 {
    let mut points = MATCH;
    if starts_word(candidate, at) {
        points += WORD_START;
    }
    if at == 0 && previous.is_none() {
        points += PREFIX;
    }
    if previous.is_some_and(|p| p + 1 == at) {
        points += CONTIGUOUS;
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(query: &str, candidate: &str) -> u32 {
        score(query, candidate).unwrap_or_else(|| panic!("{query:?} should match {candidate:?}"))
    }

    #[test]
    fn normalize_lowercases_and_strips_diacritics() {
        assert_eq!(normalize("Página"), "pagina");
        assert_eq!(normalize("PÁGINA"), "pagina");
        assert_eq!(normalize("Añadir"), "anadir");
        assert_eq!(normalize("Ü"), "u");
        assert_eq!(normalize("Ação Çà"), "acao ca");
        assert_eq!(normalize("Œuvre Straße"), "oeuvre strasse");
    }

    #[test]
    fn normalize_keeps_unfolded_characters() {
        assert_eq!(normalize("PDF-2 / x_y"), "pdf-2 / x_y");
        assert_eq!(normalize("日本"), "日本");
    }

    #[test]
    fn no_subsequence_is_no_match() {
        assert_eq!(score("xyz", "Rotate page"), None);
        assert_eq!(score("egap", "page"), None);
        assert_eq!(score("pages", "page"), None);
    }

    #[test]
    fn accent_and_case_insensitive() {
        assert!(score("pagina", "Página").is_some());
        assert!(score("pagina", "PÁGINA").is_some());
        assert!(score("PÁGINA", "pagina").is_some());
        assert_eq!(score("pagina", "Página"), score("pagina", "pagina"));
    }

    #[test]
    fn word_start_beats_mid_word() {
        // "p" at the start of "page" vs inside "Stamp".
        assert!(s("p", "Rotate page") > s("p", "Stamp"));
        assert!(s("ex", "Open export") > s("ex", "Open text"));
    }

    #[test]
    fn word_start_after_separators() {
        let mid = s("b", "xab");
        for candidate in ["xa b", "xa-b", "xa_b", "xa/b"] {
            assert!(s("b", candidate) > mid, "{candidate:?}");
        }
    }

    #[test]
    fn contiguous_beats_scattered() {
        // No separators, no prefix: only contiguity differs.
        assert!(s("ota", "xotax") > s("ota", "xoxtxa"));
        assert!(s("ota", "rotate") > s("ota", "rooxtxa"));
    }

    #[test]
    fn prefix_beats_non_prefix() {
        assert!(s("rot", "Rotate page") > s("rot", "Page rotate"));
    }

    #[test]
    fn best_alignment_is_found_not_just_the_first() {
        // The first 'p' is mid-word; a later one starts a word.
        assert!(s("pa", "Stamp page") > s("pa", "Stamp xaxa"));
    }

    #[test]
    fn later_characters_prefer_a_word_start() {
        // "ep": the 'p' of "pages" starts a word, the 'p' inside "Export"
        // does not. "xages" has no word-start 'p', so it can only score the
        // mid-word one.
        assert!(s("ep", "Export pages") > s("ep", "Export xages"));
        assert_eq!(
            s("ep", "Export pages"),
            (MATCH + WORD_START + PREFIX) + (MATCH + WORD_START)
        );
    }

    #[test]
    fn a_contiguous_run_is_not_broken_for_a_word_start() {
        // "rot" stays the prefix run of "Rotate", not 'r' + the 'o' of "options".
        assert_eq!(
            s("rot", "Rotate options"),
            (MATCH + WORD_START + PREFIX) + 2 * (MATCH + CONTIGUOUS) + run_bonus(3)
        );
    }

    #[test]
    fn a_whole_word_run_beats_any_scattered_alignment() {
        // "PDF" as a word against p, d and f each at a word start but apart
        // (the first even the candidate's prefix): before the run bonus the
        // scattered one scored 39 against the word's 19, and the palette's
        // "pdf" put "Campo de texto formulario" level with "Añadir PDF".
        assert!(s("pdf", "Añadir PDF") > s("pdf", "pon de formulario"));
        assert!(s("pdf", "Añadir PDF") > s("pdf", "Campo de texto formulario"));
        // A word prefix counts as well as a whole word.
        assert!(s("pag", "Rotate pages") > s("pag", "Pon a grid"));
    }

    #[test]
    fn a_word_start_that_strands_the_rest_is_skipped() {
        // Jumping to the word-start 'p' of "pq" would leave no 'x' after it.
        assert!(score("epx", "Expx pq").is_some());
    }

    #[test]
    fn word_start_after_punctuation_and_any_whitespace() {
        let mid = s("z", "xaz");
        for candidate in ["xa(z", "xa)z", "xa.z", "xa,z", "xa:z", "xa\tz", "xa\u{a0}z"] {
            assert!(s("z", candidate) > mid, "{candidate:?}");
        }
    }

    #[test]
    fn multi_term_query_needs_every_term() {
        assert!(score("rotar pagina", "Rotar página").is_some());
        assert!(score("pagina rotar", "Rotar página").is_some());
        assert!(score("  rotar   pagina ", "Rotar página").is_some());
        assert_eq!(score("rotar zzz", "Rotar página"), None);
    }

    #[test]
    fn empty_query_matches_everything_equally() {
        assert_eq!(score("", "Rotate page"), Some(0));
        assert_eq!(score("   ", "Rotate page"), Some(0));
        assert_eq!(score("", ""), Some(0));
    }

    #[test]
    fn non_empty_query_never_matches_empty_candidate() {
        assert_eq!(score("a", ""), None);
    }

    fn labels(items: &[&str]) -> Vec<(usize, u32)> {
        rank("pa", items, |item| vec![(*item).to_string()])
    }

    #[test]
    fn rank_drops_non_matches_and_sorts_by_score() {
        let items = ["Spam", "Rotate", "Page", "Rotate page"];
        let ranked = labels(&items);
        let order: Vec<usize> = ranked.iter().map(|(i, _)| *i).collect();
        // "Page" is a prefix match, "Rotate page" a word start, "Spam"
        // mid-word; "Rotate" has no 'p'.
        assert_eq!(order, vec![2, 3, 0]);
        assert!(ranked.windows(2).all(|w| w[0].1 >= w[1].1));
    }

    #[test]
    fn rank_ties_keep_catalog_order() {
        let items = ["Page two", "Page one", "Page three"];
        let ranked = labels(&items);
        assert_eq!(
            ranked.iter().map(|(i, _)| *i).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert!(ranked.iter().all(|(_, sc)| *sc == ranked[0].1));
    }

    #[test]
    fn rank_empty_query_keeps_everything_in_catalog_order() {
        let items = ["b", "a", "c"];
        let ranked = rank("", &items, |item| vec![(*item).to_string()]);
        assert_eq!(ranked, vec![(0, 0), (1, 0), (2, 0)]);
    }

    struct Entry {
        label: &'static str,
        english: &'static str,
        keywords: &'static [&'static str],
    }

    fn haystacks(entry: &Entry) -> Vec<String> {
        let mut all = vec![entry.label.to_string(), entry.english.to_string()];
        all.extend(entry.keywords.iter().map(|k| (*k).to_string()));
        all
    }

    #[test]
    fn rank_uses_the_best_haystack() {
        let items = [
            Entry {
                label: "Girar página",
                english: "Rotate page",
                keywords: &["turn"],
            },
            Entry {
                label: "Comprimir",
                english: "Compress",
                keywords: &["reducir tamaño", "mesh", "shrink"],
            },
        ];
        // Only the English label matches.
        assert_eq!(
            rank("rotate", &items, haystacks).first().map(|r| r.0),
            Some(0)
        );
        // Only a keyword matches (with an accent in the keyword).
        let ranked = rank("tamano", &items, haystacks);
        assert_eq!(ranked.iter().map(|r| r.0).collect::<Vec<_>>(), vec![1]);
        // The score is the best across haystacks, not the first matching one.
        // "mesh" matches "sh" mid-word first; "shrink" is a prefix.
        let best = rank("sh", &items, haystacks);
        assert_eq!(best, vec![(1, s("sh", "shrink"))]);
        assert!(s("sh", "shrink") > s("sh", "mesh"));
    }
}
