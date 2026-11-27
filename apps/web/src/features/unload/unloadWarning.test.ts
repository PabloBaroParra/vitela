import { describe, expect, it } from 'vitest';
import { unloadWarning } from './unloadWarning';

/** A window that keeps its `beforeunload` listeners, to count and fire them. */
function fakeWindow() {
  const listeners = new Set<EventListener>();
  return {
    listeners,
    addEventListener: (type: string, listener: EventListener) => void (type === 'beforeunload' && listeners.add(listener)),
    removeEventListener: (type: string, listener: EventListener) => void (type === 'beforeunload' && listeners.delete(listener)),
    /** Fires `beforeunload` as a close or reload does; says whether the browser would ask. */
    unload() {
      const event = new Event('beforeunload', { cancelable: true }) as Event & { returnValue: unknown };
      Object.defineProperty(event, 'returnValue', { value: undefined, writable: true });
      for (const listener of listeners) listener(event);
      return { prevented: event.defaultPrevented, returnValue: event.returnValue };
    },
  };
}

describe('unloadWarning', () => {
  it('listens to nothing while the document is clean, so the page can go into the back/forward cache', () => {
    const win = fakeWindow();
    const warn = unloadWarning(win);
    expect(win.listeners.size).toBe(0);
    warn(false);
    expect(win.listeners.size).toBe(0);
  });

  it('asks the browser to warn before closing or reloading while there are unsaved edits', () => {
    const win = fakeWindow();
    unloadWarning(win)(true);
    expect(win.listeners.size).toBe(1);
    // preventDefault for current browsers, returnValue for the older ones.
    expect(win.unload()).toEqual({ prevented: true, returnValue: true });
  });

  it('stops warning once the edits are saved', () => {
    const win = fakeWindow();
    const warn = unloadWarning(win);
    warn(true);
    warn(false);
    expect(win.listeners.size).toBe(0);
    expect(win.unload()).toEqual({ prevented: false, returnValue: undefined });
  });

  it('listens once, however often it is told the document is dirty', () => {
    const win = fakeWindow();
    const warn = unloadWarning(win);
    warn(true);
    warn(true);
    expect(win.listeners.size).toBe(1);
    warn(false);
    warn(true);
    expect(win.listeners.size).toBe(1);
  });
});
