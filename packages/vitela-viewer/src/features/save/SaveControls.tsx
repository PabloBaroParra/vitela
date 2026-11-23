// The Save button, what the last save did, and the questions a save can ask
// (a signature it would break, a file that changed on disk).
// Presentational: the state lives in `useSave`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { StatusLine } from '../../ui/StatusLine';
import { ChangedOnDiskDialog } from './ChangedOnDiskDialog';
import type { SaveOutcome } from './outcome';
import { SignatureDialog } from './SignatureDialog';
import type { SaveActivity, SaveSession } from './useSave';

interface SaveControlsProps {
  readonly session: SaveSession;
  readonly documentName: string;
}

export function SaveControls({ session, documentName }: SaveControlsProps) {
  const { activity, dirty, save, confirmSignatures, saveSignedCopy, cancelSignatures } = session;
  const { overwriteChanged, saveChangedCopy, cancelChanged } = session;
  const asking = activity.kind === 'confirmSignatures' || activity.kind === 'confirmChangedOnDisk';
  const busy = activity.kind === 'saving' || asking;
  const button = useRef<HTMLButtonElement>(null);

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
      {activity.kind === 'confirmSignatures' && (
        <SignatureDialog
          forCopy={activity.forCopy}
          onSaveAnyway={confirmSignatures}
          onSaveCopy={saveSignedCopy}
          onCancel={cancelSignatures}
        />
      )}
      {activity.kind === 'confirmChangedOnDisk' && (
        <ChangedOnDiskDialog onOverwrite={overwriteChanged} onSaveCopy={saveChangedCopy} onCancel={cancelChanged} />
      )}
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
      return <StatusLine>{outcomeText(activity.outcome, dirty)}</StatusLine>;
    default:
      return dirty ? <StatusLine>Unsaved changes.</StatusLine> : null;
  }
}

/**
 * What the save did. `dirty` here means edits the written file does not have:
 * a download or copy never puts the edits in the opened file, and a write
 * can miss an edit made while it ran.
 */
function outcomeText(outcome: SaveOutcome, dirty: boolean): string {
  const rest = dirty ? ' The opened file still has unsaved changes.' : '';
  switch (outcome.how) {
    case 'wrote':
      return dirty
        ? `Saved over ${outcome.name}, but edits made while saving are not in it. Unsaved changes.`
        : `Saved over ${outcome.name}.`;
    case 'delivered':
      return dirty
        ? `Saved ${outcome.name}, but edits made while saving are not in it. Unsaved changes.`
        : `Saved ${outcome.name}.`;
    case 'copied':
      return dirty
        ? `Downloaded a copy as ${outcome.name}. The opened file was not changed and still has unsaved changes.`
        : `Downloaded a copy as ${outcome.name}. The opened file was not changed.`;
    case 'cancelled':
      return 'Save cancelled.';
    case 'downloaded':
      switch (outcome.fellBack) {
        case 'permission':
          return `Permission to write over the opened file was refused, so ${outcome.name} was downloaded instead.${rest}`;
        case 'write':
          return `Could not write over the opened file, so ${outcome.name} was downloaded instead.${rest}`;
        case 'unverified':
          return `Could not check whether the opened file changed on disk, so ${outcome.name} was downloaded instead.${rest}`;
        case null:
          return `Downloaded ${outcome.name}.${rest}`;
      }
  }
}
