// The Export images button, what the last export did (with Cancel export
// while it runs), and its dialog. Presentational: the state lives in
// `useExportImages`, owned by the workspace.

import { useEffect, useRef } from 'react';
import { focusShown } from '../../ui/focusShown';
import { StatusLine } from '../../ui/StatusLine';
import { ExportImagesDialog } from './ExportImagesDialog';
import { exportProgress } from './exportText';
import { exportBusy, type ExportActivity, type ExportImagesSession } from './useExportImages';

interface ExportImagesControlsProps {
  readonly session: ExportImagesSession;
  /** Where the focus goes when the dialog closes while this button is not shown (a phone's More tools). */
  readonly focusFallback?: () => void;
}

export function ExportImagesControls({ session, focusFallback }: ExportImagesControlsProps) {
  const { activity, open, edit, cancel, stop, exportImages } = session;
  const busy = exportBusy(activity);
  const button = useRef<HTMLButtonElement>(null);

  // The dialog gives the focus back when it goes away, whatever the answer.
  const wasChoosing = useRef(false);
  useEffect(() => {
    const choosing = activity.kind === 'choosing';
    if (wasChoosing.current && !choosing) focusShown(button.current, focusFallback);
    wasChoosing.current = choosing;
  }, [activity.kind, focusFallback]);

  return (
    <>
      {/* Not `disabled`: a disabled button drops the keyboard focus to the page. */}
      <button
        ref={button}
        type="button"
        className="export-images-button"
        title="Export images"
        onClick={busy ? undefined : () => void open()}
        aria-disabled={busy}
      >
        Export images
      </button>
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
