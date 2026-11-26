// What the last watermark did, and the Add watermark dialog. It sits in the
// workspace bar, so its status survives a switch to Organize; the way in is
// the workspace's Tools menu. Presentational: the state lives in
// `useWatermark`, owned by the workspace.

import { useEffect, useRef, type RefObject } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { WatermarkDialog } from './WatermarkDialog';
import type { WatermarkActivity, WatermarkSession } from './useWatermark';

interface WatermarkControlsProps {
  readonly session: WatermarkSession;
  /** The Tools menu button that opened the dialog: it takes the focus back when the dialog closes. */
  readonly trigger: RefObject<HTMLElement | null>;
}

export function WatermarkControls({ session, trigger }: WatermarkControlsProps) {
  const { activity, edit, switchTo, pickPicture, cancel, add } = session;

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasEditing = useRef(false);
  useEffect(() => {
    const editing = activity.kind === 'editing';
    if (wasEditing.current && !editing) focusShown(trigger.current);
    wasEditing.current = editing;
  }, [activity.kind, trigger]);

  return (
    <>
      <WatermarkStatus activity={activity} />
      {activity.kind === 'editing' && (
        <WatermarkDialog
          draft={activity.draft}
          refusal={activity.refusal}
          error={activity.error}
          running={activity.running}
          onEdit={edit}
          onSwitch={switchTo}
          onPickPicture={(file) => void pickPicture(file)}
          onAdd={() => void add()}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function WatermarkStatus({ activity }: { readonly activity: WatermarkActivity }) {
  switch (activity.kind) {
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
