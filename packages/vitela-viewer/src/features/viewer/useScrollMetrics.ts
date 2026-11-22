// Tracks a scroll container's viewport: its size (ResizeObserver, plus a
// re-read after every commit) and its scroll offset (scroll events), as
// React state the viewer lays out from.

import { useCallback, useLayoutEffect, useState, type RefObject } from 'react';

export interface ScrollMetrics {
  /** clientWidth / clientHeight: the viewport, without scrollbars. */
  readonly width: number;
  readonly height: number;
  readonly top: number;
}

const read = (element: HTMLElement): ScrollMetrics => ({
  width: element.clientWidth,
  height: element.clientHeight,
  top: element.scrollTop,
});

const same = (a: ScrollMetrics, b: ScrollMetrics) =>
  a.width === b.width && a.height === b.height && a.top === b.top;

export function useScrollMetrics(ref: RefObject<HTMLElement | null>): ScrollMetrics {
  const [metrics, setMetrics] = useState<ScrollMetrics>({ width: 0, height: 0, top: 0 });

  const update = useCallback(() => {
    const element = ref.current;
    if (!element) return;
    setMetrics((prior) => {
      const next = read(element);
      return same(prior, next) ? prior : next;
    });
  }, [ref]);

  useLayoutEffect(() => {
    const element = ref.current;
    if (!element) return;
    element.addEventListener('scroll', update, { passive: true });
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(update);
    observer?.observe(element);
    return () => {
      element.removeEventListener('scroll', update);
      observer?.disconnect();
    };
  }, [ref, update]);

  // A scrollbar that appears or goes because the content grew or shrank
  // changes clientWidth without resizing the element, so no observer fires:
  // re-read after every commit. Sets state only when something changed.
  useLayoutEffect(update);

  return metrics;
}
