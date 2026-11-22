// A value that follows `value` only once it has stopped changing for
// `delayMs`. The viewer re-renders pages at a new DPI only after a zoom
// gesture settles; until then the old bitmaps are scaled.

import { useEffect, useState } from 'react';

export function useSettled<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    const timer = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, delayMs]);
  return settled;
}
