// Registers the service worker and tells the page when a new build is ready.
//
// A new deploy installs in the background, next to the running build, and
// waits. The page then offers to reload into it; nothing reloads on its own.
// The reload is held while the open document has unsaved edits (the page
// reports them through `hold`): a reload would lose them. Holds count across
// tabs: before a new build takes over, the worker asks every open tab of the
// app (HOLD_QUERY, answered here) and refuses while any of them holds edits,
// since the take-over switches all of them and drops the old build's cache.
// Closing every tab of the app also lets the waiting build take over, as
// browsers do.
//
// The banner always leads somewhere: once a new build controls this page
// (this tab's take-over, or another tab's), Reload just reloads; a waiting
// build that goes away (replaced by a newer one) takes the banner with it;
// and a take-over this tab asked for but had to drop because it held edits
// is retried as soon as the edits are saved.

export interface UpdateState {
  /** A new build is installed and waiting, or already controls this page. */
  readonly ready: boolean;
  /** The open document has unsaved edits, so the reload is held. */
  readonly held: boolean;
  /** The last take-over was refused: another tab of the app holds unsaved edits. */
  readonly blocked: boolean;
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
/** A message from the worker: its data and the ports sent with it. */
export interface WorkerMessage {
  readonly data: unknown;
  readonly ports: readonly MessagePort[];
}
export interface UpdateContainer {
  readonly controller: unknown;
  register(url: string, options?: RegistrationOptions): Promise<UpdateRegistration>;
  addEventListener(type: 'controllerchange', listener: () => void): void;
  addEventListener(type: 'message', listener: (event: WorkerMessage) => void): void;
  startMessages?(): void;
}

/** The message sw.ts answers by taking over, if no tab holds unsaved edits. */
export const SKIP_WAITING = { type: 'SKIP_WAITING' } as const;
/** sw.ts → every tab: do you hold unsaved edits? Answered with `{ held }` on the port sent along. */
export const HOLD_QUERY = { type: 'VITELA_HOLD_QUERY' } as const;
/** sw.ts → the tab that sent SKIP_WAITING: a tab holds unsaved edits, so no take-over. */
export const TAKEOVER_REFUSED = { type: 'VITELA_TAKEOVER_REFUSED' } as const;

const NONE: UpdateState = { ready: false, held: false, blocked: false };

/** For dev, tests and browsers without service workers: there is never an update. */
export const NO_UPDATES: AppUpdates = {
  subscribe: () => () => {},
  state: () => NONE,
  hold: () => {},
  apply: () => {},
};

/** Resolves once the window has loaded, so the worker's precache never competes with the app's own first load. */
export function windowLoaded(win: Pick<Window, 'addEventListener'> & { readonly document: Pick<Document, 'readyState'> }): Promise<void> {
  if (win.document.readyState === 'complete') return Promise.resolve();
  return new Promise((resolve) => win.addEventListener('load', () => resolve(), { once: true }));
}

function isMessage(data: unknown, message: { readonly type: string }): boolean {
  return typeof data === 'object' && data !== null && (data as { type?: unknown }).type === message.type;
}

/**
 * Watches for new builds. The worker registers once `loaded` resolves (the
 * window's `load` event in the app); the page answers the worker and follows
 * take-overs from the start.
 */
export function watchForUpdates(
  container: UpdateContainer,
  reload: () => void,
  loaded: Promise<unknown> = Promise.resolve(),
  url = '/sw.js',
): AppUpdates {
  const listeners = new Set<() => void>();
  let state = NONE;
  /** The installed build waiting to take over. */
  let waiting: UpdateWorker | null = null;
  /** A worker served this page; the first install's take-over is not an update. */
  let controlled = Boolean(container.controller);
  /** A new build controls this page already: only the reload is left. */
  let switched = false;
  /** This page asked for the switch: a take-over then reloads it, now or once its edits are saved. */
  let requested = false;

  function set(next: Partial<UpdateState>) {
    const merged = { ...state, ...next };
    if (merged.ready === state.ready && merged.held === state.held && merged.blocked === state.blocked) return;
    state = merged;
    for (const listener of listeners) listener();
  }
  // Only an update when a build already serves this page; the very first
  // install is the build that is running.
  function offer(worker: UpdateWorker) {
    if (!container.controller || switched) return;
    waiting = worker;
    worker.addEventListener('statechange', () => {
      // Replaced by a newer build (offered on its own) or failed: nothing waits.
      if (worker !== waiting || worker.state !== 'redundant') return;
      waiting = null;
      requested = false;
      set({ ready: false, blocked: false });
    });
    set({ ready: true, blocked: false });
  }
  function takeOver() {
    if (switched) reload();
    else waiting?.postMessage(SKIP_WAITING);
  }

  container.addEventListener('controllerchange', () => {
    const wasControlled = controlled;
    controlled = true;
    if (!wasControlled) return;
    // A new build controls this page: this tab's take-over or another tab's.
    // The page still runs the old build, whose cache is gone, so the only way
    // on is a reload.
    switched = true;
    waiting = null;
    if (requested && !state.held) {
      reload();
      return;
    }
    set({ ready: true, blocked: false });
  });

  container.addEventListener('message', (event) => {
    if (isMessage(event.data, HOLD_QUERY)) {
      event.ports[0]?.postMessage({ held: state.held });
    } else if (isMessage(event.data, TAKEOVER_REFUSED)) {
      // Held here: retried when this tab's edits are saved. Else another
      // tab's edits are in the way; Reload asks again.
      if (state.held) return;
      requested = false;
      set({ blocked: true });
    }
  });
  container.startMessages?.();

  loaded
    .then(() => container.register(url, { scope: '/' }))
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

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    state: () => state,
    hold(unsavedEdits) {
      const released = state.held && !unsavedEdits;
      set({ held: unsavedEdits });
      if (released && requested) takeOver();
    },
    apply() {
      if (state.held || (!waiting && !switched)) return;
      requested = true;
      set({ blocked: false });
      takeOver();
    },
  };
}
