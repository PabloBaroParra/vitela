// What the last extraction did, and the Extract pages dialog. Opened from
// Organize (its Extract button, on the selection), so this has no button of
// its own: it sits in the workspace bar with the other tools' status lines,
// where it stays when the screen changes. Presentational: the state lives in
// `useExtract`, owned by the workspace. The dialog hands the focus back to
// what opened it.

import { StatusLine } from '../../ui/StatusLine';
import { PageRangeDialog } from './PageRangeDialog';
import { extractRangeHint } from './extractText';
import type { ExtractActivity, ExtractSession } from './useExtract';

interface ExtractControlsProps {
  readonly session: ExtractSession;
  readonly pageCount: number;
}

export function ExtractControls({ session, pageCount }: ExtractControlsProps) {
  const { activity, cancel, extract } = session;
  return (
    <>
      <ExtractStatus activity={activity} />
      {activity.kind === 'choosing' && (
        <PageRangeDialog
          title="Extract pages"
          className="extract-dialog"
          label="Pages to extract"
          placeholder="1-3,7"
          initialText={activity.initialRange}
          hint={extractRangeHint(pageCount)}
          confirmLabel="Extract"
          runningLabel="Extracting…"
          refusal={activity.refusal}
          error={activity.error}
          running={activity.running}
          onConfirm={(range) => void extract(range)}
          onCancel={cancel}
        />
      )}
    </>
  );
}

function ExtractStatus({ activity }: { readonly activity: ExtractActivity }) {
  switch (activity.kind) {
    case 'delivering':
      return <StatusLine>Extracting…</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{`These pages could not be extracted: ${activity.message}`}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
