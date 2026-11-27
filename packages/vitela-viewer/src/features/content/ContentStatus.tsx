// The "Edit content" mode's status row under the viewer's toolbars (B26
// T-258b), there only while the mode is on. Presentational.
//
// The mode is turned on and off from the workspace's Tools menu (a
// `menuitemcheckbox`): the viewer's toolbar rows have no room left for a
// button at 1280 px or at 390 px. This row says what was selected, or how
// to select, in a polite live region, and has the one way out on the page
// itself: Done. Insert image (T-260) sits beside it: the content mode's own
// action that needs no selection.

interface ContentStatusProps {
  /** `null` while the mode is off. */
  readonly status: string | null;
  readonly onDone: () => void;
  /** Picks an image and arms the next press on a page to place it (T-260); pressed again, disarms. */
  readonly onInsertImage?: () => void;
  /** Whether an insert waits for a press on a page. */
  readonly inserting?: boolean;
}

export function ContentStatus({ status, onDone, onInsertImage, inserting = false }: ContentStatusProps) {
  if (status === null) return null;
  return (
    <div className="content-status" role="group" aria-label="Edit content">
      <p className="content-status__text" role="status" aria-live="polite">
        {status}
      </p>
      {onInsertImage && (
        <button type="button" className="content-status__insert" aria-pressed={inserting} onClick={onInsertImage}>
          Insert image
        </button>
      )}
      <button type="button" className="content-status__done" onClick={onDone}>
        Done
      </button>
    </div>
  );
}
