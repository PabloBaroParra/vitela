// Asks what to do when the opened file was changed by someone else since the
// document was opened: cancel, save a copy (a download; the file is never
// touched), or overwrite it. Presentational: it reports the answer.

import { WarningDialog } from './WarningDialog';

interface ChangedOnDiskDialogProps {
  /** Replace the file on disk, losing the change made behind our back. */
  readonly onOverwrite: () => void;
  /** Download a copy; the file on disk is left as it is. */
  readonly onSaveCopy: () => void;
  readonly onCancel: () => void;
}

export function ChangedOnDiskDialog({ onOverwrite, onSaveCopy, onCancel }: ChangedOnDiskDialogProps) {
  return (
    <WarningDialog
      icon={<ChangedFileIcon />}
      title="This file changed on disk since you opened it"
      onCancel={onCancel}
      actions={
        <>
          {/* Cancel takes the focus: the safe answer is the default one. */}
          <button type="button" autoFocus onClick={onCancel}>
            Cancel
          </button>
          <button type="button" onClick={onSaveCopy}>
            Save a copy
          </button>
          <button type="button" className="button--primary" onClick={onOverwrite}>
            Overwrite
          </button>
        </>
      }
    >
      <p>Another program changed it after it was opened. Overwriting it would lose that change.</p>
      <p className="dialog__hint">Save a copy to keep the file on disk exactly as it is now.</p>
    </WarningDialog>
  );
}

function ChangedFileIcon() {
  return (
    <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.75">
      <path d="M6 3h8l4 4v14H6V3Z" strokeLinejoin="round" />
      <path d="M14 3v4h4" strokeLinejoin="round" />
      <path d="M12 11v4" strokeLinecap="round" />
      <circle cx="12" cy="17.8" r="0.6" fill="currentColor" />
    </svg>
  );
}
