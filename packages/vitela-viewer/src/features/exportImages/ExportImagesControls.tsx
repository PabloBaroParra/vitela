// The Export images button, what the last export did, and its dialog.
// Presentational: the state lives in `useExportImages`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { StatusLine } from '../../ui/StatusLine';
import { ExportImagesDialog } from './ExportImagesDialog';
import { exportProgress } from './exportText';
import { exportBusy, type ExportActivity, type ExportImagesSession } from './useExportImages';

interface ExportImagesControlsProps {
  readonly session: ExportImagesSession;
}

export function ExportImagesControls({ session }: ExportImagesControlsProps) {
  const { activity, open, edit, cancel, exportImages } = session;
  const busy = exportBusy(activity);
  const button = useRef<HTMLButtonElement>(null);

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) button.current?.focus();
    wasChoosing.current = choosing;
  }, [activity.kind]);

  return (
    <>
      {/* Not `disabled`: a disabled button drops the keyboard focus to the page. */}
      <button
        ref={button}
        type="button"
        className="export-images-button"
        title="Export images"
        onClick={busy ? undefined : open}
        aria-disabled={busy}
      >
        Export images
      </button>
      <ExportStatus activity={activity} />
      {activity.kind === 'choosing' && (
        <ExportImagesDialog
          draft={activity.draft}
          allowed={activity.allowed}
          message={activity.message}
          checking={activity.checking}
          onChange={edit}
          onExport={() => void exportImages()}
          onCancel={cancel}
        />
      )}
    </>
  );
}


function ExportStatus({ activity }: { readonly activity: ExportActivity }) {
  switch (activity.kind) {
    case 'exporting':
      return <StatusLine>{exportProgress(activity.page, activity.index, activity.total)}</StatusLine>;
    case 'failed':
      return <StatusLine tone="error">{activity.message}</StatusLine>;
    case 'done':
      return <StatusLine>{activity.message}</StatusLine>;
    default:
      return null;
  }
}
