// The Save button, what the last save did, and the signature question.
// Presentational: the state lives in `useSave`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { StatusLine } from '../../ui/StatusLine';
import type { SaveOutcome } from './outcome';
import { SignatureDialog } from './SignatureDialog';
import type { SaveActivity, SaveSession } from './useSave';

interface SaveControlsProps {
  readonly session: SaveSession;
  readonly documentName: string;
}

export function SaveControls({ session, documentName }: SaveControlsProps) {
  const { activity, dirty, save, confirmSignatures, saveSignedCopy, cancelSignatures } = session;
  const busy = activity.kind === 'saving' || activity.kind === 'confirmSignatures';
  const button = useRef<HTMLButtonElement>(null);
  const asking = activity.kind === 'confirmSignatures';

  // The question gives the focus back when it goes away, whatever the answer.
  const wasAsking = useRef(false);
  useEffect(() => {
    if (wasAsking.current && !asking) button.current?.focus();
    wasAsking.current = asking;
  }, [asking]);

  return (
    <>
      {/* Not `disabled`: a disabled button drops the keyboard focus to the page. */}
      <button ref={button} type="button" onClick={busy ? undefined : save} aria-disabled={busy}>
        Save
      </button>
      <SaveStatus activity={activity} documentName={documentName} dirty={dirty} />
      {asking && <SignatureDialog onSaveAnyway={confirmSignatures} onSaveCopy={saveSignedCopy} onCancel={cancelSignatures} />}
    </>
  );
}

interface SaveStatusProps {
  readonly activity: SaveActivity;
  readonly documentName: string;
  readonly dirty: boolean;
}

function SaveStatus({ activity, documentName, dirty }: SaveStatusProps) {
  switch (activity.kind) {
    case 'saving':
      return <StatusLine>Saving…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`Could not save ${documentName}: ${activity.message}`}</StatusLine>;
    case 'done':
      return <StatusLine>{outcomeText(activity.outcome)}</StatusLine>;
    default:
      return dirty ? <StatusLine>Unsaved changes.</StatusLine> : null;
  }
}

function outcomeText(outcome: SaveOutcome): string {
  switch (outcome.how) {
    case 'wrote':
      return `Saved over ${outcome.name}.`;
    case 'copied':
      return `Saved a copy as ${outcome.name}. The opened file was not changed.`;
    case 'cancelled':
      return 'Save cancelled.';
    case 'downloaded':
      switch (outcome.fellBack) {
        case 'permission':
          return `Permission to write over the opened file was refused, so ${outcome.name} was downloaded instead.`;
        case 'write':
          return `Could not write over the opened file, so ${outcome.name} was downloaded instead.`;
        case null:
          return `Downloaded ${outcome.name}.`;
      }
  }
}
