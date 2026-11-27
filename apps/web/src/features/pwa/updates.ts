// Registers the service worker and tells the page when a new build is ready.
//
// A new deploy installs in the background, next to the running build, and
// waits. The page then offers to reload into it; nothing reloads on its own.
// The reload is held while the open document has unsaved edits (the page
// reports them through `hold`): a reload would lose them. Closing every tab
// of the app also lets the waiting build take over, as browsers do.

export interface UpdateState {
  /** A new build is installed and waiting. */
  readonly ready: boolean;
  /** The open document has unsaved edits, so the reload is held. */
  readonly held: boolean;
}

/** What the page reads and asks of the updates (`useSyncExternalStore`-shaped). */
export interface AppUpdates {
  subscribe(listener: () => void): () => void;
  /** The current state; the same object until it changes. */
  state(): UpdateState;
  /** Says whether the open document has unsaved edits. */
  hold(unsavedEdits: boolean): void;
  /** Switches to the waiting build and reloads, unless held. */
  apply(): void;
}

/** The parts of the browser's service worker API used here, so tests can fake them. */
export interface UpdateWorker {
  readonly state: ServiceWorkerState;
  postMessage(message: unknown): void;
  addEventListener(type: 'statechange', listener: () => void): void;
}
export interface UpdateRegistration {
  readonly waiting: UpdateWorker | null;
  readonly installing: UpdateWorker | null;
  addEventListener(type: 'updatefound', listener: () => void): void;
}
export interface UpdateContainer {
  readonly controller: unknown;
  register(url: string, options?: RegistrationOptions): Promise<UpdateRegistration>;
  addEventListener(type: 'controllerchange', listener: () => void): void;
}

/** The message sw.ts answers with `skipWaiting()`. */
export const SKIP_WAITING = { type: 'SKIP_WAITING' } as const;

const NONE: UpdateState = { ready: false, held: false };

/** For dev, tests and browsers without service workers: there is never an update. */
export const NO_UPDATES: AppUpdates = {
  subscribe: () => () => {},
  state: () => NONE,
  hold: () => {},
  apply: () => {},
};

export function watchForUpdates(container: UpdateContainer, reload: () => void, url = '/sw.js'): AppUpdates {
  const listeners = new Set<() => void>();
  let state = NONE;
  let waiting: UpdateWorker | null = null;
  /** This page asked for the switch: only then does a take-over reload it. */
  let requested = false;

  function set(next: Partial<UpdateState>) {
    const merged = { ...state, ...next };
    if (merged.ready === state.ready && merged.held === state.held) return;
    state = merged;
    for (const listener of listeners) listener();
  }
  // Only an update when a build already serves this page; the very first
  // install is the build that is running.
  function offer(worker: UpdateWorker) {
    if (!container.controller) return;
    waiting = worker;
    set({ ready: true });
  }

  container
    .register(url, { scope: '/' })
    .then((registration) => {
      if (registration.waiting) offer(registration.waiting);
      registration.addEventListener('updatefound', () => {
        const installing = registration.installing;
        installing?.addEventListener('statechange', () => {
          if (installing.state === 'installed') offer(installing);
        });
      });
    })
    // Refused (no HTTPS, private mode, blocked by a test harness): the app
    // works as before, online only.
    .catch(() => {});

  container.addEventListener('controllerchange', () => {
    if (requested && !state.held) reload();
  });

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    state: () => state,
    hold: (unsavedEdits) => set({ held: unsavedEdits }),
    apply() {
      if (!waiting || state.held) return;
      requested = true;
      waiting.postMessage(SKIP_WAITING);
    },
  };
}
