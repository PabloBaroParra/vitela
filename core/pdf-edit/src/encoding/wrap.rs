//! Greedy word wrap, independent of how a width is measured.
//!
//! Form fields and free-text annotations both wrap Helvetica text, but they
//! measure it with different tables and refuse different characters. The
//! wrapping policy — what counts as a break, what a blank paragraph does —
//! is the part that must not drift between them, so it lives here and each
//! caller supplies only `width_of`.

/// Greedily wraps `text` to lines no wider than `max_width`, one paragraph
/// per `\n` in the input, so a line break the user typed is always honored.
///
/// Words are split on single spaces; runs of spaces collapse. A single word
/// wider than `max_width` on its own gets its own line rather than being
/// split (callers that need a hard width bound break it themselves). An empty
/// paragraph yields an empty line.
///
/// `width_of` returns the width of a candidate line, or the caller's error —
/// which aborts the wrap, so a character the caller cannot measure is
/// reported instead of guessed.
pub fn wrap_greedy<E>(
    text: &str,
    max_width: f64,
    mut width_of: impl FnMut(&str) -> Result<f64, E>,
) -> Result<Vec<String>, E> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split(' ').filter(|w| !w.is_empty()) {
            let candidate = if current.is_empty() {
                word.to_string()
            } else {
                format!("{current} {word}")
            };
            if current.is_empty() || width_of(&candidate)? <= max_width {
                current = candidate;
            } else {
                lines.push(std::mem::take(&mut current));
                current = word.to_string();
            }
        }
        lines.push(current);
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One unit per character: width is just the character count.
    fn chars(line: &str) -> Result<f64, ()> {
        Ok(line.chars().count() as f64)
    }

    #[test]
    fn breaks_where_the_candidate_line_stops_fitting() {
        let lines = wrap_greedy("aaa bbb ccc ddd", 7.0, chars).unwrap();
        assert_eq!(lines, ["aaa bbb", "ccc ddd"]);
    }

    #[test]
    fn a_wider_limit_keeps_everything_on_one_line() {
        let lines = wrap_greedy("aaa bbb ccc ddd", 100.0, chars).unwrap();
        assert_eq!(lines, ["aaa bbb ccc ddd"]);
    }

    #[test]
    fn honors_explicit_newlines_including_blank_paragraphs() {
        let lines = wrap_greedy("a\n\nb", 100.0, chars).unwrap();
        assert_eq!(lines, ["a", "", "b"]);
    }

    #[test]
    fn a_word_wider_than_the_limit_gets_its_own_line() {
        let lines = wrap_greedy("hi enormousword yo", 5.0, chars).unwrap();
        assert_eq!(lines, ["hi", "enormousword", "yo"]);
    }

    #[test]
    fn runs_of_spaces_collapse() {
        let lines = wrap_greedy("a    b", 100.0, chars).unwrap();
        assert_eq!(lines, ["a b"]);
    }

    #[test]
    fn a_measurement_error_aborts_the_wrap() {
        let result = wrap_greedy("ab cd", 100.0, |line: &str| {
            if line.contains('d') {
                Err('d')
            } else {
                Ok(1.0)
            }
        });
        assert_eq!(result, Err('d'));
    }
}
