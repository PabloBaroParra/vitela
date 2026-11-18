// The document history's keys, read the same way by every screen (the
// viewer and Organize): Ctrl/⌘+Z undoes, Ctrl/⌘+Shift+Z redoes, and so does
// Ctrl+Y — except on Apple platforms, where ⌘+Y is the browser's History.
// A key is never taken from a text field (it keeps its own undo), from a
// handler that already took it, or while a modal dialog is open (the page
// behind it is inert; a key must not edit it). Pure apart from the DOM reads.

export type HistoryKey = 'undo' | 'redo';

type KeyPress = Pick<KeyboardEvent, 'key' | 'ctrlKey' | 'metaKey' | 'shiftKey' | 'altKey'>;
type KeyEvent = KeyPress & Pick<KeyboardEvent, 'target' | 'defaultPrevented'>;

/** What a key press asks of the history, if anything. `apple`: the platform is a Mac/iOS. */
export function historyKey(event: KeyPress, apple: boolean): HistoryKey | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey) return null;
  const key = event.key.toLowerCase();
  if (key === 'z') return event.shiftKey ? 'redo' : 'undo';
  if (key === 'y' && !event.shiftKey && !apple) return 'redo';
  return null;
}

/** The `aria-keyshortcuts` of Undo and Redo: the keys `historyKey` reads. */
export function historyShortcuts(apple: boolean): { readonly undo: string; readonly redo: string } {
  return apple
    ? { undo: 'Meta+Z', redo: 'Meta+Shift+Z' }
    : { undo: 'Control+Z', redo: 'Control+Shift+Z Control+Y' };
}

interface PlatformSource {
  readonly platform?: string;
  readonly userAgent?: string;
  readonly userAgentData?: { readonly platform?: string };
}

/**
 * Whether the browser runs on a Mac or an iOS device. Reads the platform,
 * never the user agent (Safari's is spoofed everywhere, test browsers too).
 */
export function isApplePlatform(source: PlatformSource | undefined = globalThis.navigator): boolean {
  if (!source) return false;
  // `||`, not `??`: Client Hints may report an empty platform.
  const platform = source.userAgentData?.platform || source.platform || '';
  return /mac|iphone|ipad|ipod/i.test(platform);
}

/** Whether a key went to a text field, which keeps its own keys. */
export function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return !!target.closest('input, textarea, select, [contenteditable]:not([contenteditable="false"])');
}

/** The app opens dialogs only as modals (`showModal`), so an open one is modal. */
export function modalOpen(target: EventTarget | null): boolean {
  const owner = target instanceof Node ? (target.ownerDocument ?? (target as Document)) : globalThis.document;
  return !!owner?.querySelector('dialog[open], [aria-modal="true"]');
}

/** The history step a key event asks for, or `null` when it is not one or is not ours to take. */
export function historyAction(event: KeyEvent, apple: boolean): HistoryKey | null {
  if (event.defaultPrevented) return null;
  const action = historyKey(event, apple);
  if (!action || isTextField(event.target) || modalOpen(event.target)) return null;
  return action;
}
