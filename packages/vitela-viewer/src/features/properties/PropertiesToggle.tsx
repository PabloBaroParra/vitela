// The Properties button, at the end of the viewer's View toolbar (zoom and
// page). Not in the workspace bar: in an embedding host whose own buttons are
// larger the bar has no width left (one more button splits it in two rows),
// and the rows under it have none either at 1280 px (the Annotations row
// would leave the Draw row). The View toolbar grows to fill its row, so the
// button takes room it already had. A phone hides it and offers Properties
// in More tools instead. The workspace owns the dialog and hands the way to
// open it down here. Presentational.

import { createContext, useContext, type RefObject } from 'react';
import { focusing } from '../annotations/focusing';

export interface PropertiesOpener {
  readonly open: () => void;
  /** The dialog is being opened or is open: the button does nothing. */
  readonly busy: boolean;
  /** Where the dialog gives the focus back. */
  readonly trigger: RefObject<HTMLButtonElement | null>;
}

/** Provided by the workspace; without one (no dialog to open) the button is not drawn. */
export const PropertiesOpenerContext = createContext<PropertiesOpener | null>(null);

export function PropertiesToggle() {
  const opener = useContext(PropertiesOpenerContext);
  if (!opener) return null;
  const { open, busy, trigger } = opener;
  return (
    // Not `disabled`: a disabled button drops the keyboard focus to the page.
    <button
      ref={trigger}
      type="button"
      className="properties-toggle"
      title="Document properties"
      aria-haspopup="dialog"
      aria-disabled={busy}
      onClick={focusing(() => {
        if (!busy) open();
      })}
    >
      Properties
    </button>
  );
}
