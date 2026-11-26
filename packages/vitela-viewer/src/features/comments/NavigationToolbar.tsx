// Previous/Next annotation, where that is, and the comments panel's toggle
// (with any other side panel's toggle the viewer puts in, `children`).
// Presentational. `aria-disabled`, not `disabled`: see `HistoryControls`.

import { useId, type ReactNode, type RefObject } from 'react';
import { focusing } from '../annotations/focusing';
import { stopLabel, type Stop } from './stops';

interface NavigationToolbarProps {
  readonly stops: readonly Stop[];
  readonly current: Stop | null;
  readonly onStep: (forward: boolean) => void;
  readonly commentsToggle: RefObject<HTMLButtonElement | null>;
  readonly commentsId: string;
  readonly commentsShown: boolean;
  /** The comments could not be listed: the panel, when it is open, says why. */
  readonly commentsFailed: boolean;
  readonly onComments: () => void;
  /** More toggles after Comments: one row less on a narrow editor than a toolbar of their own. */
  readonly children?: ReactNode;
}

const COMMENTS_FAILED = 'Could not list the comments. Open Comments to try again.';

export function NavigationToolbar(props: NavigationToolbarProps) {
  const { stops, current, onStep, commentsToggle, commentsId, commentsShown, commentsFailed, onComments, children } = props;
  const failedId = useId();
  const tellsFailure = commentsFailed && !commentsShown;
  const none = stops.length === 0;
  const place = current && `${stopLabel(current)}, ${stops.indexOf(current) + 1} of ${stops.length}`;
  return (
    <div className="viewer-toolbar annotation-nav" role="toolbar" aria-label="Annotations">
      <button type="button" aria-disabled={none} aria-label="Previous annotation" title="Previous annotation" onClick={focusing(() => onStep(false))}>
        ←
      </button>
      <button type="button" aria-disabled={none} aria-label="Next annotation" title="Next annotation" onClick={focusing(() => onStep(true))}>
        →
      </button>
      <button
        ref={commentsToggle}
        type="button"
        className="annotation-nav__comments"
        aria-expanded={commentsShown}
        aria-controls={commentsShown ? commentsId : undefined}
        aria-describedby={tellsFailure ? failedId : undefined}
        data-failed={tellsFailure || undefined}
        title={tellsFailure ? COMMENTS_FAILED : undefined}
        onClick={focusing(onComments)}
      >
        Comments
      </button>
      {children}
      <span id={failedId} className="annotation-nav__failed" role="status">
        {tellsFailure && COMMENTS_FAILED}
      </span>
      <span className="annotation-nav__status" role="status" title={place ?? undefined}>
        {place}
      </span>
    </div>
  );
}
