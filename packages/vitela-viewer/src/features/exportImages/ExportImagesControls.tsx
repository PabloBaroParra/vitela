// What the last export did (with Cancel export while it runs), and its
// dialog. It is opened from the workspace's Tools menu. Presentational: the
// state lives in `useExportImages`, owned by the workspace.

import { useEffect, useRef, type RefObject } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { ExportImagesDialog } from './ExportImagesDialog';
import { exportProgress } from './exportText';
import { type ExportActivity, type ExportImagesSession } from './useExportImages';

interface ExportImagesControlsProps {
  readonly session: ExportImagesSession;
  /** The Tools menu button that opened the dialog: it takes the focus back when the dialog closes. */
  readonly trigger: RefObject<HTMLElement | null>;
}

export function ExportImagesControls({ session, trigger }: ExportImagesControlsProps) {
  const { activity, edit, cancel, stop, exportImages } = session;

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) focusShown(trigger.current);
    wasChoosing.current = choosing;
  }, [activity.kind, trigger]);

  return (
    <>
      <ExportStatus activity={activity} />
      {activity.kind === 'exporting' && (
        <button type="button" className="export-images-stop" onClick={stop}>
          Cancel export
        </button>
      )}
      {activity.kind === 'choosing' && (
        <ExportImagesDialog
          draft={activity.draft}
          formats={activity.formats}
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
