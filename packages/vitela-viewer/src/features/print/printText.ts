// Print, as text: the key that asks for it and every sentence the feature
// shows. Pure.

type KeyPress = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>;

/**
 * Whether a key press asks to print: ⌘+P on Apple platforms (`apple`), Ctrl+P
 * elsewhere, with no other modifier — the browser's own Print key, taken so
 * the editor prints its pages instead of the page around it.
 */
export function printKey(event: KeyPress, apple: boolean): boolean {
  if (event.key.toLowerCase() !== 'p' || event.shiftKey || event.altKey) return false;
  return apple ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey;
}

/** The `aria-keyshortcuts` of Print: the key `printKey` reads. */
export function printShortcut(apple: boolean): string {
  return apple ? 'Meta+P' : 'Control+P';
}

export const PRINT_STARTING = 'Preparing to print…';
export const PRINT_CANCELLED = 'Print cancelled.';
export const NO_PAGES = 'This document has no pages to print.';

export function printProgress(page: number, total: number): string {
  return `Preparing page ${page} of ${total} for printing…`;
}

export function pageTooLarge(page: number): string {
  return `Page ${page} is too large to print.`;
}

/** `reason` is already a sentence. */
export function pageFailed(page: number, reason: string): string {
  return `Could not prepare page ${page} for printing: ${reason}`;
}

/** `reason` is already a sentence. */
export function printFailed(reason: string): string {
  return `Could not print: ${reason}`;
}
