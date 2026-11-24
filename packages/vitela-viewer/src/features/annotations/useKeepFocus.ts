// A focused control that goes away (an annotation an undo takes out, the
// toolbar of one just deleted) would leave the focus on <body>: the keyboard
// user would start over from the top of the page. Hands it over instead.

import { useLayoutEffect, useRef, type RefObject } from 'react';

/** Calls `onLost` when the element of `ref` goes away while the focus is in it. */
export function useKeepFocus(ref: RefObject<Element | null>, onLost: () => void): void {
  const lost = useRef(onLost);
  useLayoutEffect(() => {
    lost.current = onLost;
  });
  useLayoutEffect(() => {
    const element = ref.current;
    // A layout cleanup runs before the element leaves the page: the focus is still there to see.
    return () => {
      if (element?.contains(element.ownerDocument.activeElement)) lost.current();
    };
  }, [ref]);
}
