// What Print is doing (with Cancel print while the pages are drawn). Print is
// started from the workspace's Tools menu or its key. Presentational: the
// state lives in `usePrint`, owned by the workspace.

import { StatusLine } from '../../ui/StatusLine';
import { PRINT_STARTING, printProgress } from './printText';
import type { PrintActivity, PrintSession } from './usePrint';

export function PrintControls({ session }: { readonly session: PrintSession }) {
  const { activity, stop } = session;
  return (
    <>
      <PrintStatus activity={activity} />
      {activity.kind === 'preparing' && (
        <button type="button" className="print-stop" onClick={stop}>
          Cancel print
        </button>
      )}
    </>
  );
}

function PrintStatus({ activity }: { readonly activity: PrintActivity }) {
  switch (activity.kind) {
    case 'preparing':
      return <StatusLine>{activity.page === 0 ? PRINT_STARTING : printProgress(activity.page, activity.total)}</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
