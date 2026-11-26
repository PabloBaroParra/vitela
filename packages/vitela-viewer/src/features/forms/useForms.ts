// The forms panel's container: the fields, filling them in, what each page
// must paint again, and the field focused in the panel shown on its page
// (scrolled into view, clear of the floating annotation toolbar, and ringed).
// Container logic only.

import { useCallback, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import type { DocumentHandle, Engine, FormField, PageRect } from '@vitela/core';
import type { useAnnotationEdits } from '../annotations/useAnnotationEdits';
import { onPage } from '../comments/stops';
import type { OpenDocument } from '../document/useDocument';
import type { Reveal } from '../viewer/useReveal';
import { paintKeys } from './fieldValues';
import { useFormFields } from './useFormFields';
import { useFormFill } from './useFormFill';

/** Room kept around the field where there is some, in CSS px (the annotations' own). */
const REVEAL_MARGIN = { x: 16, y: 80 };

/** Where the focused field is, on its page. */
export interface FieldSpot {
  readonly pageIndex: number;
  readonly rect: PageRect;
}

export function useForms(
  engine: Engine,
  document: OpenDocument,
  editing: Pick<ReturnType<typeof useAnnotationEdits>, 'apply'>,
  reveal: Reveal,
  /** The annotation toolbar, over the foot of the pages while something is selected. */
  toolbar: RefObject<Element | null>,
) {
  const { state, retry } = useFormFields(engine, document.handle, document.formFields);
  const { allowed, errors, fill } = useFormFill(editing, document.info.formFillAllowed, state.fields);
  // The fields as first listed for this document: what its bitmaps were painted with.
  const [baseline, setBaseline] = useState<{ readonly handle: DocumentHandle; readonly keys: ReadonlyMap<number, string> } | null>(null);
  const listed = state.status === 'ready';
  if (listed && baseline?.handle !== document.handle) setBaseline({ handle: document.handle, keys: paintKeys(state.fields) });
  const baselineKeys = baseline?.handle === document.handle ? baseline.keys : null;
  const paint = useMemo(
    () => (baselineKeys ? paintKeys(state.fields, baselineKeys) : new Map<number, string>()),
    [state.fields, baselineKeys],
  );

  const pages = useRef(document.info.pages);
  useLayoutEffect(() => {
    pages.current = document.info.pages;
  });
  const [spot, setSpot] = useState<FieldSpot | null>(null);
  const focusField = useCallback(
    (field: FormField, rect: PageRect) => {
      const page = pages.current[field.pageIndex];
      if (!page) return;
      const shown = { pageIndex: field.pageIndex, rect: onPage(rect, page) };
      setSpot(shown);
      reveal(shown.pageIndex, shown.rect, REVEAL_MARGIN, toolbar.current);
    },
    [reveal, toolbar],
  );
  const blurField = useCallback(() => setSpot(null), []);

  return { state, retry, allowed, errors, fill, paintKeys: paint, spot, focusField, blurField };
}
