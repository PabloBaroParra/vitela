// Scrolls each search match the user moves to into view — from the layout,
// so a page that is not mounted yet works too. Every move is a new search
// state; a zoom is not, and must not scroll back to the match.

import { useLayoutEffect, useRef, type RefObject } from 'react';
import { currentHit, type SearchState } from '../search/searchState';
import { boundingBox, toCssRects } from '../text/pageRects';
import type { Layout } from './layout';
import { revealScroll } from './reveal';

/** Room kept around a search match scrolled into view, in CSS px. */
const REVEAL_MARGIN = 48;

export function useRevealMatch(scroller: RefObject<HTMLElement | null>, search: SearchState, layout: Layout, scale: number): void {
  const view = useRef({ layout, scale });
  useLayoutEffect(() => {
    view.current = { layout, scale };
  });
  useLayoutEffect(() => {
    const element = scroller.current;
    const hit = currentHit(search);
    if (!element || !hit) return;
    const { layout, scale } = view.current;
    const slot = layout.slots[hit.pageIndex];
    const box = slot && boundingBox(toCssRects(hit.rects, scale));
    if (!slot || !box) return;
    const target = { ...box, left: slot.left + box.left, top: slot.top + box.top };
    const viewport = {
      left: element.scrollLeft,
      top: element.scrollTop,
      width: element.clientWidth,
      height: element.clientHeight,
    };
    const scroll = revealScroll(viewport, target, REVEAL_MARGIN);
    if (!scroll) return;
    element.scrollLeft = scroll.x;
    element.scrollTop = scroll.y;
  }, [scroller, search]);
}
