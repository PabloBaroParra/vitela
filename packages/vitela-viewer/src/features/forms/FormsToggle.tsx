// The button that shows and hides the forms panel; it sits beside the
// Comments toggle. Presentational.

import type { RefObject } from 'react';
import { focusing } from '../annotations/focusing';

interface FormsToggleProps {
  readonly toggle: RefObject<HTMLButtonElement | null>;
  readonly panelId: string;
  readonly shown: boolean;
  readonly onToggle: () => void;
}

export function FormsToggle({ toggle, panelId, shown, onToggle }: FormsToggleProps) {
  return (
    <button
      ref={toggle}
      type="button"
      className="forms-toggle"
      aria-expanded={shown}
      aria-controls={shown ? panelId : undefined}
      onClick={focusing(onToggle)}
    >
      Forms
    </button>
  );
}
