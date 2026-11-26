// The React half of a snapshot job (`snapshotJob.ts`): one job at a time,
// dropped when the document on screen changes or the editor goes away, and
// stopped on request. Export images and Print own one each. Container logic
// only, no markup.

import { useEffect, useMemo, useRef } from 'react';
import type { JobGuard } from './snapshotJob';

export interface SnapshotJob {
  /** Takes the one-at-a-time slot; `false` while another job holds it. */
  readonly claim: () => boolean;
  /** Gives the slot back. */
  readonly release: () => void;
  /** A guard for a job on the document of `generation`; it starts unstopped. */
  readonly guard: (generation: number) => JobGuard;
  /** Whether the document of `generation` is still the one shown, in a mounted editor. */
  readonly isCurrent: (generation: number) => boolean;
  /** Asks the running job to stop. */
  readonly stop: () => void;
}

/**
 * `generation` is the document on screen. `dispose`, which must be stable,
 * is called when it changes and when the editor goes away: what the feature
 * still holds of a job (a print frame) goes with it.
 */
export function useSnapshotJob(generation: number | undefined, dispose?: () => void): SnapshotJob {
  /** The document on screen: a job for an old document is dropped. */
  const shown = useRef(generation);
  /** False once the editor is gone: a running job stops at its next step. */
  const mounted = useRef(true);
  const busy = useRef(false);
  const stopRequested = useRef(false);

  useEffect(() => {
    shown.current = generation;
    busy.current = false;
    dispose?.();
  }, [generation, dispose]);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      dispose?.();
    };
  }, [dispose]);

  return useMemo(() => {
    const isCurrent = (running: number) => mounted.current && shown.current === running;
    return {
      claim: () => {
        if (busy.current) return false;
        busy.current = true;
        return true;
      },
      release: () => {
        busy.current = false;
      },
      guard: (running: number) => {
        stopRequested.current = false;
        return { current: () => isCurrent(running), stopped: () => stopRequested.current };
      },
      isCurrent,
      stop: () => {
        stopRequested.current = true;
      },
    };
  }, []);
}
