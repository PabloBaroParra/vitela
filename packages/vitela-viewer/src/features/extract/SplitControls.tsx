// What the last split did (with Cancel split while it runs), and the Split
// dialog. Opened from Organize's Split button, so this has no button of its
// own: it sits in the workspace bar with the other tools' status lines,
// where it stays (Cancel split included) when the screen changes.
// Presentational: the state lives in `useSplit`, owned by the workspace.
// The dialog hands the focus back to what opened it.

import { StatusLine } from '../../ui/StatusLine';
import { PageRangeDialog } from './PageRangeDialog';
import { splitHint, splitProgress } from './extractText';
import type { SplitActivity, SplitSession } from './useSplit';

interface SplitControlsProps {
  readonly session: SplitSession;
  readonly pageCount: number;
}

export function SplitControls({ session, pageCount }: SplitControlsProps) {
  const { activity, cancel, stop, split } = session;
  return (
    <>
      <SplitStatus activity={activity} />
      {activity.kind === 'splitting' && (
        <button type="button" className="split-stop" onClick={stop}>
          Cancel split
        </button>
      )}
      {activity.kind === 'choosing' && (
        <PageRangeDialog
          title="Split PDF"
          className="split-dialog"
          label="Split after page"
          placeholder="3,7"
          hint={splitHint(pageCount)}
          confirmLabel="Split"
          runningLabel="Splitting…"
          refusal={activity.refusal}
          error={activity.error}
          running={activity.running}
          onConfirm={(cuts) => void split(cuts)}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function SplitStatus({ activity }: { readonly activity: SplitActivity }) {
  switch (activity.kind) {
    case 'splitting':
      return <StatusLine>{splitProgress(activity.index, activity.total)}</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
