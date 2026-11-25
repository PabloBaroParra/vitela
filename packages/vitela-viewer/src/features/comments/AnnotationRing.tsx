// Marks an annotation shown on its page that is not selected there (one the
// file carries, or any where annotating is off). Inert. Presentational.

import type { PageRect } from '@vitela/core';

export function AnnotationRing({ rect, scale }: { readonly rect: PageRect; readonly scale: number }) {
  const { left, top, width, height } = rect;
  return (
    <div
      className="annotation-ring"
      aria-hidden="true"
      style={{ left: left * scale, top: top * scale, width: width * scale, height: height * scale }}
    />
  );
}
