// Paints one rendered page. Presentational: it is handed the pixels.

import { useLayoutEffect, useRef, useState } from 'react';
import type { RenderedPage } from '../../core/types';
import { paintPage } from './paint';

interface PageCanvasProps {
  readonly page: RenderedPage;
  readonly label: string;
}

export function PageCanvas({ page, label }: PageCanvasProps) {
  const canvas = useRef<HTMLCanvasElement>(null);
  // Exposed as data-painted so E2E tests can wait for real pixels.
  const [painted, setPainted] = useState<RenderedPage | null>(null);

  useLayoutEffect(() => {
    if (!canvas.current) return;
    paintPage(canvas.current, page);
    setPainted(page);
  }, [page]);

  return (
    <canvas
      ref={canvas}
      className="page-canvas"
      role="img"
      aria-label={label}
      data-painted={painted === page ? 'true' : 'false'}
    />
  );
}
