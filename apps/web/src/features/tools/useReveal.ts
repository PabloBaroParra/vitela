// Scroll reveal: hides an element (`reveal--pending`) until it scrolls into
// view, then lets the CSS fade it in. Content stays visible by default; the
// element is only hidden once an observer exists to show it again, so
// without IntersectionObserver (old browsers, tests) nothing is ever hidden.

import { useCallback, type RefCallback } from 'react';

export const PENDING_CLASS = 'reveal--pending';

export function useReveal<T extends Element>(): RefCallback<T> {
  return useCallback((node: T | null) => {
    if (!node || typeof IntersectionObserver === 'undefined') return;
    node.classList.add(PENDING_CLASS);
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        node.classList.remove(PENDING_CLASS);
        observer.disconnect();
      },
      { threshold: 0.05, rootMargin: '0px 0px -40px 0px' },
    );
    observer.observe(node);
    return () => {
      observer.disconnect();
      node.classList.remove(PENDING_CLASS);
    };
  }, []);
}
