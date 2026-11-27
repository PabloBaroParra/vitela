// The "Edit content" mode's status row under the viewer's toolbars (B26
// T-258b), there only while the mode is on. Presentational.
//
// The mode is turned on and off from the workspace's Tools menu (a
// `menuitemcheckbox`): the viewer's toolbar rows have no room left for a
// button at 1280 px or at 390 px. This row says what was selected, or how
// to select, in a polite live region, and has the one way out on the page
// itself: Done. Insert text (T-261) and Insert image (T-260) sit beside it:
// the content mode's own actions that need no selection.

interface ContentStatusProps {
  /** `null` while the mode is off. */
  readonly status: string | null;
  readonly onDone: () => void;
  /** Arms the next press on a page to open a new line of text there (T-261); pressed again, disarms. */
  readonly onInsertText?: () => void;
  /** Whether Insert text waits for a press on a page. */
  readonly insertingText?: boolean;
  /** Picks an image and arms the next press on a page to place it (T-260); pressed again, disarms. */
  readonly onInsertImage?: () => void;
  /** Whether an insert waits for a press on a page. */
  readonly inserting?: boolean;
}

export function ContentStatus({ status, onDone, onInsertText, insertingText = false, onInsertImage, inserting = false }: ContentStatusProps) {
  if (status === null) return null;
  return (
    <div className="content-status" role="group" aria-label="Edit content">
      <p className="content-status__text" role="status" aria-live="polite">
        {status}
      </p>
      {onInsertText && (
        <button type="button" className="content-status__insert" aria-pressed={insertingText} onClick={onInsertText}>
          Insert text
        </button>
      )}
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
