// What the last protection did, and its dialog. It is opened from the
// workspace's Tools menu. Presentational: the state lives in `useProtect`,
// owned by the workspace.

import { useEffect, useRef, type RefObject } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { ProtectDialog } from './ProtectDialog';
import { protectStatus } from './protectText';
import { type ProtectActivity, type ProtectSession } from './useProtect';

interface ProtectControlsProps {
  readonly session: ProtectSession;
  readonly documentName: string;
  /** The Tools menu button that opened the dialog: it takes the focus back when the dialog closes. */
  readonly trigger: RefObject<HTMLElement | null>;
}

export function ProtectControls({ session, documentName, trigger }: ProtectControlsProps) {
  const { activity, cancel, protect } = session;

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) focusShown(trigger.current);
    wasChoosing.current = choosing;
  }, [activity.kind, trigger]);

  return (
    <>
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
