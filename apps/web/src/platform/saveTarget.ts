// A new file to save into, chosen by the user: Chromium's `showSaveFilePicker`.
// Other browsers have no such picker; the caller downloads instead.

import type { WritableFileHandle } from './fileHandle';

interface SavePickerWindow {
  showSaveFilePicker?: (options: unknown) => Promise<WritableFileHandle>;
}

/** Whether this browser can ask the user where to save. */
export function canPickSaveTarget(win: object = window): boolean {
  return typeof (win as SavePickerWindow).showSaveFilePicker === 'function';
}

/**
 * Shows the browser's save dialog; `null` when the user dismisses it. Needs
 * the user's click still to be fresh, so call it before any slow work.
 */
export async function pickSaveTarget(suggestedName: string, win: object = window): Promise<WritableFileHandle | null> {
  const picker = (win as SavePickerWindow).showSaveFilePicker;
  if (!picker) return null;
  try {
    return await picker.call(win, {
      suggestedName,
      types: [{ description: 'PDF', accept: { 'application/pdf': ['.pdf'] } }],
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') return null;
    throw error;
  }
}
