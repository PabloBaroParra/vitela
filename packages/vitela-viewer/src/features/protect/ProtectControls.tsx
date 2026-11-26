// The Protect button, what the last protection did, and its dialog.
// Presentational: the state lives in `useProtect`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { ProtectDialog } from './ProtectDialog';
import { protectStatus } from './protectText';
import { protectBusy, type ProtectActivity, type ProtectSession } from './useProtect';

interface ProtectControlsProps {
  readonly session: ProtectSession;
  readonly documentName: string;
  /** Where the focus goes when the dialog closes while this button is not shown (a phone's More tools). */
  readonly focusFallback?: () => void;
}

export function ProtectControls({ session, documentName, focusFallback }: ProtectControlsProps) {
  const { activity, open, cancel, protect } = session;
  const busy = protectBusy(activity);
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
        className="protect-button"
        title="Protect with a password"
        onClick={busy ? undefined : () => void open()}
        aria-disabled={busy}
      >
        Protect
      </button>
      <ProtectStatus activity={activity} documentName={documentName} />
      {activity.kind === 'choosing' && (
        <ProtectDialog
          refusal={activity.refusal}
          signaturesWillBreak={activity.signaturesWillBreak}
          error={activity.error}
          running={activity.running}
          onProtect={(passwords) => void protect(passwords)}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function ProtectStatus({ activity, documentName }: { readonly activity: ProtectActivity; readonly documentName: string }) {
  switch (activity.kind) {
    case 'delivering':
      return <StatusLine>Protecting…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not protect ${documentName}: ${activity.message}`}</StatusLine>;
    case 'done':
      return <StatusLine>{protectStatus(activity.outcome)}</StatusLine>;
    default:
      return null;
  }
}
