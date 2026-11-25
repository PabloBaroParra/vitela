import type { MouseEvent } from 'react';

/**
 * Safari does not focus a clicked button; a dialog gives the focus back to
 * what had it, and a toolbar that goes away hands over the focus it held.
 */
export const focusing = (action: () => void) => (event: MouseEvent<HTMLElement>) => {
  event.currentTarget.focus({ preventScroll: true });
  action();
};
