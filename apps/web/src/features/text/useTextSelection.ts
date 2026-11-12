// Selecting text on a page: a press sets the anchor, every move asks the
// engine for the text and rects between the anchor and the pointer, and a
// double click selects a word. The selection stays on the page the press
// landed on. Container logic only, no markup.
//
// The engine answers in its own time while the pointer keeps moving, so at
// most one request is in flight: moves that arrive meanwhile collapse into
// the latest one, and an answer that is no longer the latest is dropped.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { TEXT_EXTRACTION_DENIED, errorKind } from '../../core/errors';
import type { DocumentHandle, Engine, PagePoint, PageRects, TextSelection } from '../../core/types';

export interface PageSelection {
  readonly pageIndex: number;
  /** The selected text as the core reads it (`textIn`): what Copy copies. */
  readonly text: string;
  readonly rects: PageRects;
}

/** What a text layer calls on pointer input over page `pageIndex`. */
export interface TextInput {
  start(pageIndex: number, point: PagePoint): void;
  extend(point: PagePoint): void;
  finish(): void;
  selectWord(pageIndex: number, point: PagePoint): void;
}

export function useTextSelection(engine: Engine, document: DocumentHandle, onDenied: () => void) {
  const [selection, setSelection] = useState<PageSelection | null>(null);
  /** Where the current drag started; `null` when no drag is under way. */
  const anchor = useRef<{ pageIndex: number; point: PagePoint } | null>(null);
  /** Bumped per request and per reset: only the latest answer is shown. */
  const latest = useRef(0);
  const inFlight = useRef(false);
  /** The request to send once the one in flight answers. */
  const queued = useRef<(() => void) | null>(null);
  // Read through a ref, so a new callback never changes `input`.
  const denied = useRef(onDenied);
  useEffect(() => {
    denied.current = onDenied;
  });

  useEffect(
    () => () => {
      latest.current++;
      queued.current = null;
    },
    [],
  );

  const request = useCallback(
    (pageIndex: number, ask: () => Promise<TextSelection>) => {
      const id = ++latest.current;
      const send = () => {
        inFlight.current = true;
        ask()
          .then(
            ({ text, rects }) => {
              if (id === latest.current) setSelection(text === '' ? null : { pageIndex, text, rects });
            },
            (error: unknown) => {
              if (id !== latest.current) return;
              setSelection(null);
              if (errorKind(error) === TEXT_EXTRACTION_DENIED) denied.current();
            },
          )
          .finally(() => {
            inFlight.current = false;
            const next = queued.current;
            queued.current = null;
            next?.();
          });
      };
      if (inFlight.current) queued.current = send;
      else send();
    },
    [],
  );

  const reset = useCallback(() => {
    latest.current++;
    queued.current = null;
    setSelection(null);
  }, []);

  const start = useCallback(
    (pageIndex: number, point: PagePoint) => {
      anchor.current = { pageIndex, point };
      reset();
    },
    [reset],
  );
  const extend = useCallback(
    (point: PagePoint) => {
      const from = anchor.current;
      if (from) request(from.pageIndex, () => engine.selectText(document, from.pageIndex, from.point, point));
    },
    [engine, document, request],
  );
  const finish = useCallback(() => {
    anchor.current = null;
  }, []);
  const selectWord = useCallback(
    (pageIndex: number, point: PagePoint) => {
      anchor.current = null;
      request(pageIndex, () => engine.selectWord(document, pageIndex, point));
    },
    [engine, document, request],
  );
  // One object for the life of the document, so memoized pages keep it.
  const input = useMemo<TextInput>(
    () => ({ start, extend, finish, selectWord }),
    [start, extend, finish, selectWord],
  );

  const clear = useCallback(() => {
    anchor.current = null;
    reset();
  }, [reset]);

  return { selection, input, clear };
}
