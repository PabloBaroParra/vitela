// The Compress button, what the last compression did, and its dialog.
// Presentational: the state lives in `useCompress`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { CompressDialog } from './CompressDialog';
import { compressStatus } from './compressText';
import { compressBusy, type CompressActivity, type CompressSession } from './useCompress';

interface CompressControlsProps {
  readonly session: CompressSession;
  readonly documentName: string;
  /** Where the focus goes when the dialog closes while this button is not shown (a phone's More tools). */
  readonly focusFallback?: () => void;
}

export function CompressControls({ session, documentName, focusFallback }: CompressControlsProps) {
  const { activity, open, select, cancel, compress } = session;
  const busy = compressBusy(activity);
  const button = useRef<HTMLButtonElement>(null);

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) focusShown(button.current, focusFallback);
    wasChoosing.current = choosing;
  }, [activity.kind, focusFallback]);

  return (
    <>
      {/* Not `disabled`: a disabled button drops the keyboard focus to the page. */}
      <button
        ref={button}
        type="button"
        className="compress-button"
        title="Compress"
        onClick={busy ? undefined : () => void open()}
        aria-disabled={busy}
      >
        Compress
      </button>
      <CompressStatus activity={activity} documentName={documentName} />
      {activity.kind === 'choosing' && (
        <CompressDialog
          presets={activity.presets}
          preset={activity.preset}
          refusal={activity.refusal}
          signaturesWillBreak={activity.signaturesWillBreak}
          onSelect={select}
          onCompress={() => void compress()}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function CompressStatus({ activity, documentName }: { readonly activity: CompressActivity; readonly documentName: string }) {
  switch (activity.kind) {
    case 'compressing':
      return <StatusLine>Compressing…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not compress ${documentName}: ${activity.message}`}</StatusLine>;
    case 'done':
      return <StatusLine>{compressStatus(activity.result)}</StatusLine>;
    default:
      return null;
  }
}
