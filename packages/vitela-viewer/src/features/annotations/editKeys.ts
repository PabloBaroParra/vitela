// The keys that edit the annotation that has the focus: arrows move it,
// Alt+arrows resize it (right and down grow, left and up shrink, the
// top-left corner stays), Shift takes the large step, Delete or Backspace
// removes it and Escape lets it go. Ctrl/⌘ combinations are left to the
// browser and to the history keys. Pure.

/** One arrow press, in points. */
export const NUDGE_PT = 1;
/** One Shift+arrow press: the native shells' Nudge step (`NUDGE_PT`). */
export const NUDGE_LARGE_PT = 12;

export type EditKey =
  | { readonly kind: 'move'; readonly dx: number; readonly dy: number }
  | { readonly kind: 'resize'; readonly dw: number; readonly dh: number }
  | { readonly kind: 'remove' }
  | { readonly kind: 'deselect' };

type KeyPress = Pick<KeyboardEvent, 'key' | 'shiftKey' | 'altKey' | 'ctrlKey' | 'metaKey'>;

const DIRECTIONS: Record<string, readonly [number, number]> = {
  ArrowLeft: [-1, 0],
  ArrowRight: [1, 0],
  ArrowUp: [0, -1],
  ArrowDown: [0, 1],
};

/** The `aria-keyshortcuts` of an annotation that can be edited from the keyboard. */
export const EDIT_SHORTCUTS = [
  'ArrowLeft ArrowRight ArrowUp ArrowDown',
  'Shift+ArrowLeft Shift+ArrowRight Shift+ArrowUp Shift+ArrowDown',
  'Alt+ArrowLeft Alt+ArrowRight Alt+ArrowUp Alt+ArrowDown',
  'Delete Backspace Escape',
].join(' ');

/** What a key press asks of the focused annotation, if anything. */
export function editKey(event: KeyPress): EditKey | null {
  if (event.ctrlKey || event.metaKey) return null;
  const direction = DIRECTIONS[event.key];
  if (direction) {
    const step = event.shiftKey ? NUDGE_LARGE_PT : NUDGE_PT;
    const [x, y] = direction;
    return event.altKey ? { kind: 'resize', dw: x * step, dh: y * step } : { kind: 'move', dx: x * step, dy: y * step };
  }
  if (event.shiftKey || event.altKey) return null;
  if (event.key === 'Delete' || event.key === 'Backspace') return { kind: 'remove' };
  if (event.key === 'Escape') return { kind: 'deselect' };
  return null;
}
