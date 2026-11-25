// Scrolls a box of a page into view — from the layout, so a page that is not
// mounted yet works too.

import { useCallback, useLayoutEffect, useRef, type RefObject } from 'react';
import type { Layout, Rect } from './layout';
import { fittingMargins, revealScroll, type Margins } from './reveal';

/**
 * Scrolls so `box` (points on page `pageIndex` as drawn) shows, if it does
 * not, with `margin` CSS px around it. `foot` floats over the bottom of the
 * pages: the box is brought above it, keeping as much of the margin as fits.
 */
export type Reveal = (pageIndex: number, box: Rect, margin: number | Margins, foot?: Element | null) => void;

/** How much of the bottom of `element` is under `foot`. */
function covered(element: Element, foot: Element | null | undefined): number {
  if (!foot) return 0;
  return Math.max(0, element.getBoundingClientRect().bottom - foot.getBoundingClientRect().top);
}

/** The same function for the viewer's life: it reads the layout and zoom of when it is called. */
export function useReveal(scroller: RefObject<HTMLElement | null>, layout: Layout, scale: number): Reveal {
  const view = useRef({ layout, scale });
  useLayoutEffect(() => {
    view.current = { layout, scale };
  });
  return useCallback(
    (pageIndex, box, margin, foot) => {
      const element = scroller.current;
      const { layout, scale } = view.current;
      const slot = layout.slots[pageIndex];
      if (!element || !slot) return;
      const target = {
        left: slot.left + box.left * scale,
        top: slot.top + box.top * scale,
        width: box.width * scale,
        height: box.height * scale,
      };
      const viewport = {
        left: element.scrollLeft,
        top: element.scrollTop,
        width: element.clientWidth,
        height: Math.max(0, element.clientHeight - covered(element, foot)),
      };
      const scroll = revealScroll(viewport, target, typeof margin === 'number' ? margin : fittingMargins(viewport, target, margin));
      if (!scroll) return;
      element.scrollLeft = scroll.x;
      element.scrollTop = scroll.y;
    },
    [scroller],
  );
}
