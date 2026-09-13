//! Putting pages *into* a page tree, at a page index.
//!
//! Split out of [`crate::graft`] and [`crate::create_blank`] because both had
//! the same shortcut written into them twice: treat the root `/Pages` node's
//! `/Kids` as the document's page list, insert into it, and set `/Count` to
//! its length. That holds only while the tree is one node deep. A page tree
//! is a tree (PDF 32000-1:2008 section 7.7.3.2): a kid is a page *or* another
//! `/Pages` node, `/Count` is the number of pages in the whole subtree, and a
//! page index therefore says nothing about a position in any single `/Kids`.
//!
//! On a nested destination the shortcut inserted at the wrong place and left
//! a `/Count` smaller than the number of pages actually in the file — the
//! kind of damage a reader shows as missing pages.

use lopdf::{Document as LopdfRawDocument, Object, ObjectId};

use crate::error::ManipError;

/// How deep a page tree is walked before it is treated as malformed. Mirrors
/// the cap [`crate::page_graph`] puts on the inheritance walk, and for the
/// same reason: a cycle must not hang the import.
const MAX_PAGE_TREE_DEPTH: usize = 32;

/// Inserts `pages` into the tree rooted at `root`, contiguously and in order,
/// so that the first of them ends up at 0-based page index `index`; an
/// `index` at or past the last page appends.
///
/// Returns the `/Pages` node the pages were attached to — the caller's job is
/// to set each page's `/Parent` to it, because a page whose `/Parent` is not
/// the node listing it resolves inherited attributes against the wrong
/// branch.
///
/// Every `/Count` from `root` down to that node is recomputed by walking the
/// subtree rather than incremented, so a file that arrived with a wrong
/// `/Count` does not keep it.
pub(crate) fn insert_pages_at(
    doc: &mut LopdfRawDocument,
    root: ObjectId,
    index: usize,
    pages: &[ObjectId],
) -> Result<ObjectId, ManipError> {
    let (path, kid_index) = locate(doc, root, index)?;
    let parent = *path
        .last()
        .expect("the path always holds at least the root");

    let node = doc.get_dictionary_mut(parent)?;
    let mut kids = node
        .get(b"Kids")
        .and_then(|kids| kids.as_array())
        .cloned()
        .unwrap_or_default();
    let at = kid_index.min(kids.len());
    for (offset, &page_id) in pages.iter().enumerate() {
        kids.insert(at + offset, Object::Reference(page_id));
    }
    node.set("Kids", kids);

    for &node_id in path.iter().rev() {
        let count = subtree_page_count(doc, node_id, MAX_PAGE_TREE_DEPTH) as i64;
        doc.get_dictionary_mut(node_id)?.set("Count", count);
    }

    Ok(parent)
}

/// Resolves a page index to the node that should list the new page and the
/// position inside that node's `/Kids`, together with the path of `/Pages`
/// nodes walked to reach it (root first), whose `/Count` the insertion
/// invalidates.
fn locate(
    doc: &LopdfRawDocument,
    root: ObjectId,
    index: usize,
) -> Result<(Vec<ObjectId>, usize), ManipError> {
    let mut path = vec![root];
    let mut node = root;
    let mut offset = index;

    for _ in 0..MAX_PAGE_TREE_DEPTH {
        let kids: Vec<ObjectId> = doc
            .get_dictionary(node)
            .map_err(|_| ManipError::MalformedPageTree)?
            .get(b"Kids")
            .and_then(|kids| kids.as_array())
            .map(|kids| {
                kids.iter()
                    .filter_map(|kid| kid.as_reference().ok())
                    .collect()
            })
            .unwrap_or_default();

        let mut consumed = 0usize;
        let mut descend = None;
        for (position, &kid) in kids.iter().enumerate() {
            if offset == consumed {
                return Ok((path, position));
            }
            let is_node = doc
                .get_dictionary(kid)
                .ok()
                .and_then(|dict| dict.get(b"Type").ok())
                .and_then(|value| value.as_name().ok())
                == Some(b"Pages");
            let size = if is_node {
                subtree_page_count(doc, kid, MAX_PAGE_TREE_DEPTH)
            } else {
                1
            };
            // Strictly inside this kid's span: only a subtree can be entered,
            // and a page's span is one, so this is always a `/Pages` node.
            if offset < consumed + size {
                descend = Some(kid);
                offset -= consumed;
                break;
            }
            consumed += size;
        }

        match descend {
            // Past every page this node holds: append to it.
            None => return Ok((path, kids.len())),
            Some(kid) => {
                path.push(kid);
                node = kid;
            }
        }
    }

    Err(ManipError::MalformedPageTree)
}

/// The number of pages in the subtree rooted at `node`, counted by walking it
/// rather than by trusting its `/Count`.
fn subtree_page_count(doc: &LopdfRawDocument, node: ObjectId, depth: usize) -> usize {
    if depth == 0 {
        return 0;
    }
    let Ok(dict) = doc.get_dictionary(node) else {
        return 0;
    };
    let Ok(kids) = dict.get(b"Kids").and_then(|kids| kids.as_array()) else {
        return 0;
    };
    kids.iter()
        .filter_map(|kid| kid.as_reference().ok())
        .map(|kid| {
            let is_node = doc
                .get_dictionary(kid)
                .ok()
                .and_then(|dict| dict.get(b"Type").ok())
                .and_then(|value| value.as_name().ok())
                == Some(b"Pages");
            if is_node {
                subtree_page_count(doc, kid, depth - 1)
            } else {
                1
            }
        })
        .sum()
}
