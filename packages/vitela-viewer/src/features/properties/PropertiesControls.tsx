// What the last properties change did, and the Document properties dialog.
// It sits in the workspace bar, so its status survives a switch to
// Organize; the way in is `PropertiesToggle` (the end of the viewer's View toolbar)
// or a phone's More tools. Presentational: the state lives in
// `useDocumentProperties`, owned by the workspace.

import { useEffect, useRef, type RefObject } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { PropertiesDialog } from './PropertiesDialog';
import type { PropertiesActivity, PropertiesSession } from './useDocumentProperties';

interface PropertiesControlsProps {
  readonly session: PropertiesSession;
  /** The Properties button, which takes the focus back when the dialog closes. */
  readonly trigger: RefObject<HTMLButtonElement | null>;
  /** Where the focus goes instead while that button is not shown (a phone's More tools). */
  readonly focusFallback?: () => void;
}

export function PropertiesControls({ session, trigger, focusFallback }: PropertiesControlsProps) {
  const { activity, edit, cancel, apply } = session;

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasEditing = useRef(false);
  useEffect(() => {
    const editing = activity.kind === 'editing';
    if (wasEditing.current && !editing) focusShown(trigger.current, focusFallback);
    wasEditing.current = editing;
  }, [activity.kind, trigger, focusFallback]);

  return (
    <>
      <PropertiesStatus activity={activity} />
      {activity.kind === 'editing' && (
        <PropertiesDialog
          draft={activity.draft}
          refusal={activity.refusal}
          error={activity.error}
          running={activity.running}
          onEdit={edit}
          onApply={() => void apply()}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function PropertiesStatus({ activity }: { readonly activity: PropertiesActivity }) {
  switch (activity.kind) {
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
