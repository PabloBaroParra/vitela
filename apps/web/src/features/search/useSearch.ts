// Searching the open document: asks the engine (the core's exact,
// case-sensitive search), keeps only the answer to the latest query, and
// moves between matches. Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { TEXT_EXTRACTION_DENIED, errorKind, errorMessage } from '../../core/errors';
import type { DocumentHandle, Engine } from '../../core/types';
import { IDLE, step, type SearchState } from './searchState';

/**
 * `onDenied` runs when the engine refuses the search because the document
 * withholds its text — the viewer then blocks every text feature.
 */
export function useSearch(engine: Engine, document: DocumentHandle, onDenied: () => void) {
  const [state, setState] = useState<SearchState>(IDLE);
  /** Bumped per query: an answer to an older one is dropped. */
  const latest = useRef(0);

  // Nothing set after unmount.
  useEffect(
    () => () => {
      latest.current++;
    },
    [],
  );

  const search = useCallback(
    (query: string) => {
      const id = ++latest.current;
      if (query === '') {
        setState(IDLE);
        return;
      }
      setState({ kind: 'searching', query });
      engine.search(document, query).then(
        (hits) => {
          if (id === latest.current) setState({ kind: 'found', query, hits, current: 0 });
        },
        (error: unknown) => {
          if (id !== latest.current) return;
          if (errorKind(error) === TEXT_EXTRACTION_DENIED) {
            setState(IDLE);
            onDenied();
          } else {
            setState({ kind: 'failed', query, message: errorMessage(error) });
          }
        },
      );
    },
    [engine, document, onDenied],
  );

  const next = useCallback(() => setState((current) => step(current, 1)), []);
  const previous = useCallback(() => setState((current) => step(current, -1)), []);

  /** Enter in the search box: a new query searches, the same one moves on. */
  const submit = useCallback(
    (query: string, backwards: boolean) => {
      if (state.kind === 'found' && state.query === query) setState(step(state, backwards ? -1 : 1));
      else search(query);
    },
    [state, search],
  );

  const clear = useCallback(() => search(''), [search]);

  return { state, submit, next, previous, clear };
}
