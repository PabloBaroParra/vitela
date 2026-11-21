// The layout a text box's text will have, asked of the core while it is
// typed (`freeTextLayout`: the same layout the saved appearance gets), so
// the dialog can show it — and show at once a character Helvetica cannot
// show, which the core refuses with `EncodingGap`. Asked once typing pauses;
// an answer for text that has since changed is dropped.

import { useEffect, useState } from 'react';
import { errorMessage } from '@vitela/core';
import type { Engine, FreeTextLayout } from '@vitela/core';
import { isBlank } from './drawing';

/** How long typing must pause before the layout is asked for. */
export const PREVIEW_DELAY_MS = 120;

export interface FreeTextPreview {
  readonly layout: FreeTextLayout | null;
  /** Why the core will not lay this text out. */
  readonly error: string | null;
}

const NONE: FreeTextPreview = { layout: null, error: null };

export function useFreeTextPreview(engine: Engine, text: string, widthPt: number, heightPt: number): FreeTextPreview {
  const [preview, setPreview] = useState<{ readonly text: string; readonly result: FreeTextPreview }>({ text: '', result: NONE });
  useEffect(() => {
    if (isBlank(text)) return;
    let live = true;
    const timer = setTimeout(() => {
      engine.freeTextLayout(text, widthPt, heightPt).then(
        (layout) => live && setPreview({ text, result: { layout, error: null } }),
        (error: unknown) => live && setPreview({ text, result: { layout: null, error: errorMessage(error) } }),
      );
    }, PREVIEW_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
    };
  }, [engine, text, widthPt, heightPt]);
  // Only an answer for the text now in the box counts.
  return preview.text === text && !isBlank(text) ? preview.result : NONE;
}
