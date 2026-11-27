// Gives the focus back to a control that may not be on screen (one a narrow
// layout hides cannot take it), so the focus goes where the user can see it.

/**
 * Focuses `element` when it is shown, else calls `fallback`. A browser
 * without `checkVisibility` (jsdom) counts the element as shown.
 */
export function focusShown(element: HTMLElement | null, fallback?: () => void): void {
  if (element && (element.checkVisibility?.() ?? true)) element.focus();
  else fallback?.();
}
