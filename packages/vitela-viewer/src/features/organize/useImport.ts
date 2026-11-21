// "Add PDFs": prepares every picked file in turn (asking a locked one for its
// own password), then adds them all as ONE import — one undo step for the
// whole pick, as on the native shells. Container logic only, no markup.
//
// The rules are the core's: whether a source opens, may be copied out of and
// can be grafted is asked by `prepareImport`; whether this document accepts
// pages at all by `importPrepared` (and, before the picker, `importRefusal`).
// A pick abandoned half-way (a cancelled password, a refused file, the screen
// going away) discards what it had prepared, so the engine keeps nothing.

import { useCallback, useEffect, useRef, useState } from 'react';
import { PASSWORD_REQUIRED, WRONG_PASSWORD, errorKind, errorMessage } from '@vitela/core';
import type { Engine, PreparedImport } from '@vitela/core';

/** A prepared source and the file it came from. */
export interface PreparedSource extends PreparedImport {
  readonly name: string;
}

export type ImportActivity =
  | { readonly kind: 'idle' }
  | { readonly kind: 'preparing'; readonly name: string }
  | {
      readonly kind: 'password';
      readonly name: string;
      /** The last password tried did not open the file. */
      readonly wrong: boolean;
      /** A password is being checked right now. */
      readonly checking: boolean;
    }
  | { readonly kind: 'importing' }
  | { readonly kind: 'added'; readonly message: string }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: ImportActivity = { kind: 'idle' };

/**
 * `importAll` adds the prepared sources to the document and says whether it
 * took them; when it did not, they are discarded here.
 */
export function useImport(engine: Engine, importAll: (sources: readonly PreparedSource[]) => Promise<boolean>) {
  const [activity, setActivity] = useState<ImportActivity>(IDLE);
  const running = useRef(false);
  const alive = useRef(true);
  /** Answers the open password prompt: the password, or `null` to give up the pick. */
  const answer = useRef<((password: string | null) => void) | null>(null);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      answer.current?.(null);
    };
  }, []);

  const add = useCallback(
    async (files: readonly File[]) => {
      if (running.current || files.length === 0) return;
      running.current = true;
      const prepared: PreparedSource[] = [];
      const abandon = () => {
        for (const { id } of prepared) void engine.discardImport(id).catch(() => {});
      };
      try {
        for (const file of files) {
          const source = await prepare(file);
          if (!source) return abandon();
          prepared.push(source);
        }
        if (!alive.current) return abandon();
        setActivity({ kind: 'importing' });
        if (!(await importAll(prepared))) {
          abandon();
          setActivity(IDLE);
          return;
        }
        setActivity({ kind: 'added', message: addedMessage(prepared) });
      } finally {
        running.current = false;
      }

      /** One file, asking for its password as often as needed; `null` when the pick ends here. */
      async function prepare(file: File): Promise<PreparedSource | null> {
        let password: string | undefined;
        for (;;) {
          if (!alive.current) return null;
          if (password === undefined) setActivity({ kind: 'preparing', name: file.name });
          try {
            // Read afresh each time: the engine takes the bytes by transfer.
            const bytes = new Uint8Array(await file.arrayBuffer());
            return { ...(await engine.prepareImport(bytes, password)), name: file.name };
          } catch (error) {
            const kind = errorKind(error);
            if (kind !== PASSWORD_REQUIRED && kind !== WRONG_PASSWORD) {
              setActivity({ kind: 'failed', message: `Could not add ${file.name}: ${errorMessage(error)}` });
              return null;
            }
            const wrong = password !== undefined;
            const given = await new Promise<string | null>((resolve) => {
              answer.current = resolve;
              setActivity({ kind: 'password', name: file.name, wrong, checking: false });
            });
            answer.current = null;
            if (given === null) {
              setActivity(IDLE);
              return null;
            }
            password = given;
            setActivity({ kind: 'password', name: file.name, wrong, checking: true });
          }
        }
      }
    },
    [engine, importAll],
  );

  const submitPassword = useCallback((password: string) => answer.current?.(password), []);
  const cancelPassword = useCallback(() => answer.current?.(null), []);

  return { activity, busy: activity.kind !== 'idle' && activity.kind !== 'added' && activity.kind !== 'failed', add, submitPassword, cancelPassword };
}

function addedMessage(sources: readonly PreparedSource[]): string {
  const pages = sources.reduce((sum, source) => sum + source.pageCount, 0);
  const from = sources.length === 1 ? sources[0]!.name : `${sources.length} PDFs`;
  const added = `Added ${pages} ${pages === 1 ? 'page' : 'pages'} from ${from}.`;
  const warnings = sources.flatMap((source) => source.warnings);
  return warnings.length === 0 ? added : `${added} Not everything came across: ${warnings.join('; ')}.`;
}
