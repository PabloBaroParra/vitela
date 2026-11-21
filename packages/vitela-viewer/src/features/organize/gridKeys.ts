// The Organize grid's keyboard: what a key press means, given where the
// focus is. Everything the pointer does has a key: arrows move the focus
// (Shift extends the selection), Ctrl+Shift+arrow (Cmd+Shift on a Mac) moves
// the selected pages, Delete removes them. Pure.
//
// Ctrl+Shift rather than Alt: Alt+Left is "Back" in Chromium and Firefox.

export interface KeyPress {
  readonly key: string;
  readonly shiftKey: boolean;
  readonly ctrlKey: boolean;
  readonly metaKey: boolean;
  readonly altKey: boolean;
}

export interface GridPosition {
  /** The active card, or `null` when nothing is selected. */
  readonly focus: number | null;
  readonly columns: number;
  readonly pageCount: number;
}

export type GridKeyAction =
  | { readonly kind: 'focus'; readonly index: number; readonly extend: boolean }
  /** Move the selection by `delta` places. */
  | { readonly kind: 'move'; readonly delta: number }
  | { readonly kind: 'delete' };

export function gridKeyAction(press: KeyPress, { focus, columns, pageCount }: GridPosition): GridKeyAction | null {
  if (pageCount === 0 || press.altKey) return null;
  const step = stepOf(press.key, columns, focus ?? 0, pageCount);
  const command = press.ctrlKey || press.metaKey;
  if (command) {
    if (!press.shiftKey || step === null || focus === null || press.key === 'Home' || press.key === 'End') return null;
    return { kind: 'move', delta: step };
  }
  if (press.key === 'Delete' || press.key === 'Backspace') return focus === null ? null : { kind: 'delete' };
  if (step === null) return null;
  const from = focus ?? 0;
  const index = focus === null ? 0 : Math.max(0, Math.min(pageCount - 1, from + step));
  return { kind: 'focus', index, extend: press.shiftKey };
}

/** How far a navigation key goes from `from`; `null` for any other key. */
function stepOf(key: string, columns: number, from: number, pageCount: number): number | null {
  switch (key) {
    case 'ArrowLeft':
      return -1;
    case 'ArrowRight':
      return 1;
    case 'ArrowUp':
      return -columns;
    case 'ArrowDown':
      return columns;
    case 'Home':
      return -from;
    case 'End':
      return pageCount - 1 - from;
    default:
      return null;
  }
}
