// Opening a document: reads the picked file, hands its bytes to the engine
// and tracks what the UI should show. Container logic only, no markup.

import { useCallback, useRef, useState } from 'react';
import type { Engine } from '../../core/types';

export type DocumentState =
  | { readonly status: 'empty' }
  | { readonly status: 'opening'; readonly name: string }
  | { readonly status: 'failed'; readonly name: string; readonly message: string }
  | {
      readonly status: 'open';
      readonly name: string;
      readonly pageCount: number;
      /** Bumps on every successful open, so views re-render a same-named file. */
      readonly generation: number;
    };

export function useDocument(engine: Engine) {
  const [state, setState] = useState<DocumentState>({ status: 'empty' });
  const latest = useRef(0);

  const open = useCallback(
    (file: File) => {
      const attempt = ++latest.current;
      const current = () => attempt === latest.current;
      setState({ status: 'opening', name: file.name });
      file
        .arrayBuffer()
        .then((buffer) => engine.open(new Uint8Array(buffer)))
        .then(
          ({ pageCount }) => {
            if (current()) setState({ status: 'open', name: file.name, pageCount, generation: attempt });
          },
          (error: unknown) => {
            const message = error instanceof Error ? error.message : String(error);
            if (current()) setState({ status: 'failed', name: file.name, message });
          },
        );
    },
    [engine],
  );

  return { state, open };
}
