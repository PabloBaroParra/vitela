//! Shared split boundaries for shells that ask where to cut a document.

use super::parse_page_selection;

/// Resolve one-based cut positions into ascending zero-based page positions.
pub fn resolve_split_cuts(typed: &str, total_pages: u32) -> Result<Vec<u32>, String> {
    if total_pages < 2 {
        return Err("A document of one page cannot be split.".to_owned());
    }
    if typed.trim().is_empty() {
        return Err("Type where to cut, for example 3 to split after page 3.".to_owned());
    }
    let cuts = parse_page_selection(typed, total_pages).map_err(|error| error.to_string())?;
    if cuts.contains(&(total_pages - 1)) {
        return Err(format!(
            "There is nothing after page {total_pages}, so the document cannot be split there."
        ));
    }
    Ok(cuts)
}

/// Inclusive zero-based ranges partitioning the pages at ascending cut positions.
pub fn split_parts(cuts: &[u32], total_pages: u32) -> Vec<(u32, u32)> {
    let mut parts = Vec::with_capacity(cuts.len() + 1);
    let mut first = 0;
    for &cut in cuts {
        parts.push((first, cut));
        first = cut + 1;
    }
    parts.push((first, total_pages - 1));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cuts_tile_every_page_once() {
        let cuts = resolve_split_cuts("7,3,3", 10).unwrap();
        assert_eq!(split_parts(&cuts, 10), vec![(0, 2), (3, 6), (7, 9)]);
    }

    #[test]
    fn cutting_after_final_page_is_refused() {
        assert!(resolve_split_cuts("10", 10)
            .unwrap_err()
            .contains("nothing after page 10"));
    }
}
