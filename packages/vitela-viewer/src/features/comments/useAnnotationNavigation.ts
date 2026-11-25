// Previous/Next annotation and going to a comment: scrolls it into view and
// selects it when it can be edited (one of the session's, with Select in
// hand); else it is only marked on its page. Nothing here edits, so it works
// where annotating is forbidden. Container logic only.
//
// The place is kept by key: an annotation selected by hand becomes it, and it
// is still where the next step goes on from once the selection is let go. If
// the annotation goes away, the next step starts over.

import { useCallback, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import type { Annotation, PageRect, PageSize } from '@vitela/core';
import type { AnnotationInput } from '../annotations/useAnnotationEditing';
import type { Reveal } from '../viewer/useReveal';
import { onPage, sessionKey, stepStop, stopsOf, type CommentEntry, type Stop } from './stops';

/** Room kept around it where there is some, in CSS px. */
const REVEAL_MARGIN = { x: 16, y: 80 };

/** Where an annotation is, on its page. */
interface Spot {
  readonly pageIndex: number;
  readonly rect: PageRect;
}

interface Place {
  readonly key: string | null;
  /** Drawn on the page: it could not be selected instead. */
  readonly marked: boolean;
}

export function useAnnotationNavigation(
  pages: readonly PageSize[],
  annotations: readonly Annotation[],
  entries: readonly CommentEntry[],
  selected: Annotation | null,
  /** Selecting; `null` while a tool is armed or the document forbids annotating. */
  input: Pick<AnnotationInput, 'select'> | null,
  deselect: () => void,
  reveal: Reveal,
  /** The annotation toolbar, over the foot of the pages while something is selected. */
  toolbar: RefObject<Element | null>,
) {
  const stops = useMemo(() => stopsOf(annotations, entries), [annotations, entries]);
  const [place, setPlace] = useState<Place>({ key: null, marked: false });
  if (selected && (place.marked || place.key !== sessionKey(selected.id))) setPlace({ key: sessionKey(selected.id), marked: false });
  const current = stops.find(({ key }) => key === place.key) ?? null;

  const spotOf = ({ pageIndex, rect }: Stop): Spot | null => {
    const page = pages[pageIndex];
    return page ? { pageIndex, rect: onPage(rect, page) } : null;
  };
  // Once committed: selecting it may have brought up the toolbar it must clear.
  const [target, setTarget] = useState<Spot | null>(null);
  const revealed = useRef<Spot | null>(null);
  useLayoutEffect(() => {
    if (!target || revealed.current === target) return;
    revealed.current = target;
    reveal(target.pageIndex, target.rect, REVEAL_MARGIN, toolbar.current);
  }, [target, reveal, toolbar]);

  const visit = (stop: Stop | null | undefined) => {
    if (!stop) return;
    const selects = input !== null && stop.annotationId !== null;
    if (selects) input.select(stop.annotationId!);
    else deselect();
    setPlace({ key: stop.key, marked: !selects });
    setTarget(spotOf(stop));
  };
  const unmark = useCallback(() => setPlace((shown) => (shown.marked ? { ...shown, marked: false } : shown)), []);

  return {
    stops,
    current,
    /** What to mark on its page. */
    marked: place.marked && !selected && current ? spotOf(current) : null,
    unmark,
    step: (forward: boolean) => visit(stepStop(stops, current?.key ?? null, forward)),
    visitKey: (key: string) => visit(stops.find((stop) => stop.key === key)),
  };
}
