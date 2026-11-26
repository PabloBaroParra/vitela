// Compress: a preset, then the run, then the copy — the order of the other
// shells. Both gates (a document that cannot be compressed, a signature that
// would break) are asked as the dialog opens, so the dialog can say why
// before anything runs. The copy is delivered only when it came out smaller:
// the core promises "never larger", not "ever smaller", and a file handed
// over under a "-compressed" name that is not smaller would mislead.
//
// Nothing is reopened afterwards: the open document, its pending edits and
// its undo history are exactly as they were; there is simply a smaller copy.
// Container logic only, no markup.

import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, type CompressPreset, type Engine } from '@vitela/core';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery } from '../save/delivery';
import { sentence } from '../../ui/text';
import { DEFAULT_PRESET, compressedName, type CompressResult } from './compressText';

export type CompressActivity =
  | { readonly kind: 'idle' }
  /** Asking the gates before the dialog opens. */
  | { readonly kind: 'checking' }
  /** The dialog is open. */
  | {
      readonly kind: 'choosing';
      readonly presets: readonly CompressPreset[];
      readonly preset: CompressPreset;
      /** Why it cannot run, as a sentence; `null` when it can. */
      readonly refusal: string | null;
      /** Confirming is the user's yes to breaking the file's signature. */
      readonly signaturesWillBreak: boolean;
    }
  | { readonly kind: 'compressing' }
  | { readonly kind: 'done'; readonly result: CompressResult }
  | { readonly kind: 'failed'; readonly message: string };

const IDLE: CompressActivity = { kind: 'idle' };

/** From the click until the dialog closes or the run ends: Compress cannot be started again. */
export function compressBusy(activity: CompressActivity): boolean {
  return activity.kind === 'checking' || activity.kind === 'choosing' || activity.kind === 'compressing';
}

export type CompressSession = ReturnType<typeof useCompress>;

export function useCompress(
  engine: Engine,
  document: OpenDocument | null,
  delivery: Delivery,
  /** Settles once the edits the editing screen has asked for so far have ended. */
  settleEdits?: () => Promise<unknown>,
) {
  const generation = document?.generation;
  const [activity, setActivity] = useState<CompressActivity>(IDLE);
  const [forGeneration, setForGeneration] = useState(generation);
  /** The document on screen: an answer for an old document changes nothing. */
  const shown = useRef(generation);
  /** True from the click until the dialog closes or the run ends: one at a time. */
  const busy = useRef(false);

  // A new document starts clean (adjusting state while rendering).
  if (forGeneration !== generation) {
    setForGeneration(generation);
    setActivity(IDLE);
  }
  useEffect(() => {
    shown.current = generation;
    busy.current = false;
  }, [generation]);

  const open = useCallback(async () => {
    if (!document || busy.current) return;
    busy.current = true;
    const asking = document.generation;
    const current = () => shown.current === asking;
    setActivity({ kind: 'checking' });
    let presets: readonly CompressPreset[] = [];
    let refusal: string | null = null;
    let signaturesWillBreak = false;
    try {
      presets = await engine.compressPresets();
      const refused = await engine.compressionRefusal(document.handle);
      if (refused) refusal = sentence(refused.message);
      // A failed question refuses rather than guessing "unsigned": a guess
      // could hand over a copy whose signature silently broke.
      else signaturesWillBreak = await engine.compressionInvalidatesSignatures(document.handle);
    } catch (error) {
      refusal = sentence(errorMessage(error));
    }
    if (!current()) return;
    const preset = presets.includes(DEFAULT_PRESET) ? DEFAULT_PRESET : (presets[0] ?? DEFAULT_PRESET);
    setActivity({ kind: 'choosing', presets, preset, refusal, signaturesWillBreak });
  }, [engine, document]);

  const select = useCallback((preset: CompressPreset) => {
    setActivity((a) => (a.kind === 'choosing' && a.refusal === null ? { ...a, preset } : a));
  }, []);

  const cancel = useCallback(() => {
    setActivity((a) => (a.kind === 'choosing' ? IDLE : a));
    busy.current = false;
  }, []);

  const compress = useCallback(async () => {
    if (!document || activity.kind !== 'choosing' || activity.refusal !== null) return;
    const { preset, signaturesWillBreak } = activity;
    const running = document.generation;
    const current = () => shown.current === running;
    setActivity({ kind: 'compressing' });
    try {
      // The edits asked for before the click go in.
      await settleEdits?.();
      // The dialog showed the signature warning and its button said so:
      // confirming it is the acknowledgement, never a second prompt.
      const { bytes, ...report } = await engine.saveCompressedToBytes(document.handle, preset, signaturesWillBreak);
      if (!current()) return;
      if (!report.reduced) {
        setActivity({ kind: 'done', result: { kind: 'noGain', report } });
        return;
      }
      const outcome = await delivery.deliver({ bytes, name: compressedName(document.name), destination: { kind: 'newFile' } });
      if (!current()) return;
      // A new file is never compared with the disk, so this cannot happen; say nothing was written if it does.
      setActivity({
        kind: 'done',
        result: { kind: 'reduced', report, outcome: outcome.how === 'changedOnDisk' ? { how: 'cancelled' } : outcome },
      });
    } catch (error) {
      if (current()) setActivity({ kind: 'failed', message: errorMessage(error) });
    } finally {
      if (current()) busy.current = false;
    }
  }, [engine, document, delivery, settleEdits, activity]);

  return { activity, open, select, cancel, compress };
}
