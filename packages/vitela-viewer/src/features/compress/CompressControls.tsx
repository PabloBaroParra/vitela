// What the last compression did, and its dialog. It is opened from the
// workspace's Tools menu. Presentational: the state lives in `useCompress`,
// owned by the workspace.

import { useEffect, useRef, type RefObject } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { CompressDialog } from './CompressDialog';
import { compressStatus } from './compressText';
import { type CompressActivity, type CompressSession } from './useCompress';

interface CompressControlsProps {
  readonly session: CompressSession;
  readonly documentName: string;
  /** The Tools menu button that opened the dialog: it takes the focus back when the dialog closes. */
  readonly trigger: RefObject<HTMLElement | null>;
}

export function CompressControls({ session, documentName, trigger }: CompressControlsProps) {
  const { activity, select, cancel, compress } = session;

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) focusShown(trigger.current);
    wasChoosing.current = choosing;
  }, [activity.kind, trigger]);

  return (
    <>
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
