// Scrolls each search match the user moves to into view. Every move is a new
// search state; a zoom is not, and must not scroll back to the match.

import { useLayoutEffect } from 'react';
import { currentHit, type SearchState } from '../search/searchState';
import { boundingBox, toCssRects } from '../text/pageRects';
import type { Reveal } from './useReveal';

/** Room kept around a search match scrolled into view, in CSS px. */
const REVEAL_MARGIN = 48;

export function useRevealMatch(reveal: Reveal, search: SearchState): void {
  useLayoutEffect(() => {
    const hit = currentHit(search);
    // At a scale of 1: the match's box in points.
    const box = hit && boundingBox(toCssRects(hit.rects, 1));
    if (hit && box) reveal(hit.pageIndex, box, REVEAL_MARGIN);
  }, [reveal, search]);
}
