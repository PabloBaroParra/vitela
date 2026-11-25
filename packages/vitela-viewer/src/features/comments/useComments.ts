// Every comment of the document, asked of the core again after each edit
// (they come back in its order, merged with the session's notes). One request
// at a time: edits made while one is being answered are covered by a single
// request after it, and only the answer to the latest is shown. The list of
// before an edit stays on screen until then, unless the pages changed.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage } from '@vitela/core';
import type { Comment, DocumentHandle, Engine } from '@vitela/core';

export type CommentsState =
  | { readonly status: 'loading' | 'ready'; readonly comments: readonly Comment[] }
  | { readonly status: 'failed'; readonly comments: readonly Comment[]; readonly message: string };

const LOADING: CommentsState = { status: 'loading', comments: [] };

interface Request {
  readonly engine: Engine;
  readonly handle: DocumentHandle;
  /** What sits at each position: the answer fits only these pages. */
  readonly structure: string;
}

/** `revision` is anything that changes identity with each edit. */
export function useComments(engine: Engine, handle: DocumentHandle, revision: unknown, structure: string) {
  const [answer, setAnswer] = useState<{ readonly to: Request; readonly state: CommentsState } | null>(null);
  const [attempt, setAttempt] = useState(0);
  const wanted = useRef<Request | null>(null);
  const running = useRef(false);

  useEffect(() => {
    wanted.current = { engine, handle, structure };
    if (running.current) return;
    running.current = true;
    void (async () => {
      let request: Request | null;
      while ((request = wanted.current)) {
        const state = await load(request);
        if (wanted.current !== request) continue;
        setAnswer({ to: request, state });
        break;
      }
      running.current = false;
    })();
  }, [engine, handle, revision, structure, attempt]);
  useEffect(
    () => () => {
      wanted.current = null;
    },
    [],
  );

  const retry = useCallback(() => {
    setAnswer(null);
    setAttempt((count) => count + 1);
  }, []);
  const fits = answer && answer.to.engine === engine && answer.to.handle === handle && answer.to.structure === structure;
  return { state: fits ? answer.state : LOADING, retry };
}

async function load({ engine, handle }: Request): Promise<CommentsState> {
  try {
    return { status: 'ready', comments: await engine.comments(handle) };
  } catch (error) {
    return { status: 'failed', comments: [], message: errorMessage(error) };
  }
}
