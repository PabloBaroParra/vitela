// The Save button, what the last save did, and the signature question.
// Container: the state lives in `useSave`.

import type { Engine } from '../../core/types';
import { StatusLine } from '../../ui/StatusLine';
import type { OpenDocument } from '../document/useDocument';
import { SignatureDialog } from './SignatureDialog';
import { useSave, type SaveActivity } from './useSave';

interface SaveControlsProps {
  readonly engine: Engine;
  readonly document: OpenDocument;
  /** Replaces the delivery of the bytes; for tests. */
  readonly deliver?: Parameters<typeof useSave>[2];
}

export function SaveControls({ engine, document, deliver }: SaveControlsProps) {
  const { activity, dirty, save, confirmSignatures, cancelSignatures } = useSave(engine, document, deliver);
  const busy = activity.kind === 'saving' || activity.kind === 'confirmSignatures';
  return (
    <>
      <button type="button" onClick={save} disabled={busy}>
        Save
      </button>
      <SaveStatus activity={activity} documentName={document.name} dirty={dirty} />
      {activity.kind === 'confirmSignatures' && (
        <SignatureDialog onProceed={confirmSignatures} onCancel={cancelSignatures} />
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
    case 'done': {
      const { outcome } = activity;
      if (outcome.how === 'wrote') return <StatusLine>{`Saved over ${outcome.name}.`}</StatusLine>;
      return (
        <StatusLine>
          {outcome.fellBack
            ? `Could not write over the opened file, so ${outcome.name} was downloaded instead.`
            : `Downloaded ${outcome.name}.`}
        </StatusLine>
      );
    }
    default:
      return dirty ? <StatusLine>Unsaved changes.</StatusLine> : null;
  }
}
