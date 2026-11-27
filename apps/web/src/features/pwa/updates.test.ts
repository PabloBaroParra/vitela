import { describe, expect, it, vi } from 'vitest';
import {
  HOLD_QUERY,
  NO_UPDATES,
  TAKEOVER_REFUSED,
  watchForUpdates,
  windowLoaded,
  type UpdateContainer,
  type UpdateRegistration,
  type UpdateWorker,
} from './updates';

class FakeTarget {
  private readonly listeners = new Map<string, Array<(event: never) => void>>();
  addEventListener(type: string, listener: (event: never) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  fire(type: string, event?: unknown) {
    for (const listener of this.listeners.get(type) ?? []) listener(event as never);
  }
}

class FakeWorker extends FakeTarget implements UpdateWorker {
  state: ServiceWorkerState = 'installing';
  readonly messages: unknown[] = [];
  postMessage(message: unknown) {
    this.messages.push(message);
  }
  become(state: ServiceWorkerState) {
    this.state = state;
    this.fire('statechange');
  }
}

class FakeRegistration extends FakeTarget implements UpdateRegistration {
  waiting: FakeWorker | null = null;
  installing: FakeWorker | null = null;
}

const SKIP = { type: 'SKIP_WAITING' };

/** A page's service worker container; `controlled` says whether a worker already serves it. */
function setup({ controlled = true, waiting = false, loaded = Promise.resolve() as Promise<unknown> } = {}) {
  const registration = new FakeRegistration();
  if (waiting) registration.waiting = new FakeWorker();
  const container = new (class extends FakeTarget implements UpdateContainer {
    controller: object | null = controlled ? {} : null;
    readonly registered: Array<[string, RegistrationOptions | undefined]> = [];
    register(url: string, options?: RegistrationOptions) {
      this.registered.push([url, options]);
      return Promise.resolve(registration);
    }
  })();
  const reload = vi.fn();
  const updates = watchForUpdates(container, reload, loaded);
  /** A new build finishes installing in the background. */
  async function newBuildInstalls() {
    await vi.waitFor(() => expect(container.registered).toHaveLength(1));
    await Promise.resolve();
    const worker = new FakeWorker();
    registration.installing = worker;
    registration.fire('updatefound');
    worker.become('installed');
    return worker;
  }
  /** The worker asks this tab whether it holds unsaved edits; resolves with the answer. */
  function askHold(): Promise<unknown> {
    const { port1, port2 } = new MessageChannel();
    const answer = new Promise((resolve) => {
      port1.onmessage = (event) => {
        port1.close();
        resolve(event.data);
      };
    });
    container.fire('message', { data: HOLD_QUERY, ports: [port2] });
    return answer;
  }
  /** The worker refuses this tab's take-over: some tab holds unsaved edits. */
  function refuse() {
    container.fire('message', { data: TAKEOVER_REFUSED, ports: [] });
  }
  return { container, registration, reload, updates, newBuildInstalls, askHold, refuse };
}

describe('watchForUpdates', () => {
  it('registers /sw.js for the whole site', async () => {
    const { container } = setup();
    await vi.waitFor(() => expect(container.registered).toEqual([['/sw.js', { scope: '/' }]]));
  });

  it('registers only once the window has loaded', async () => {
    let load!: () => void;
    const { container } = setup({ loaded: new Promise<void>((resolve) => (load = resolve)) });
    await Promise.resolve();
    await Promise.resolve();
    expect(container.registered).toEqual([]);
    load();
    await vi.waitFor(() => expect(container.registered).toHaveLength(1));
  });

  it('has nothing to offer until a new build is installed', async () => {
    const { updates } = setup();
    await Promise.resolve();
    expect(updates.state()).toEqual({ ready: false, held: false, blocked: false });
  });

  it('offers the update once a new build is installed, and tells subscribers', async () => {
    const { updates, newBuildInstalls } = setup();
    const listener = vi.fn();
    updates.subscribe(listener);
    await newBuildInstalls();
    expect(updates.state()).toEqual({ ready: true, held: false, blocked: false });
    expect(listener).toHaveBeenCalled();
  });

  it('offers a build that was already waiting when the page loaded', async () => {
    const { updates } = setup({ waiting: true });
    await vi.waitFor(() => expect(updates.state().ready).toBe(true));
  });

  it('offers nothing on the first install: that build is the one already running', async () => {
    const { updates, newBuildInstalls } = setup({ controlled: false });
    await newBuildInstalls();
    expect(updates.state().ready).toBe(false);
  });

  it('keeps the same state object until something changes', async () => {
    const { updates } = setup();
    expect(updates.state()).toBe(updates.state());
  });

  it('activates the waiting build and reloads once it has taken over', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    updates.apply();
    expect(worker.messages).toEqual([SKIP]);
    expect(reload).not.toHaveBeenCalled();
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('never reloads while the open document has unsaved edits', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    updates.hold(true);
    expect(updates.state()).toEqual({ ready: true, held: true, blocked: false });
    updates.apply();
    expect(worker.messages).toEqual([]);
    container.fire('controllerchange');
    expect(reload).not.toHaveBeenCalled();
  });

  it('does not reload if edits come in between the click and the take-over', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    await newBuildInstalls();
    updates.apply();
    updates.hold(true);
    container.fire('controllerchange');
    expect(reload).not.toHaveBeenCalled();
  });

  it('can update once the edits are saved', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    updates.hold(true);
    updates.hold(false);
    updates.apply();
    expect(worker.messages).toEqual([SKIP]);
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does not reload when another tab activated the new build, but Reload then reloads', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    container.fire('controllerchange');
    expect(reload).not.toHaveBeenCalled();
    // The banner stays, and its Reload no longer waits on a worker that is
    // not waiting any more.
    expect(updates.state().ready).toBe(true);
    updates.apply();
    expect(worker.messages).toEqual([]);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('offers the reload when another tab took over before this one saw the new build', async () => {
    const { container, updates, reload } = setup();
    await vi.waitFor(() => expect(container.registered).toHaveLength(1));
    container.fire('controllerchange');
    expect(updates.state().ready).toBe(true);
    updates.apply();
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('keeps Reload held after another tab took over while this one has unsaved edits', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    await newBuildInstalls();
    updates.hold(true);
    container.fire('controllerchange');
    updates.apply();
    expect(reload).not.toHaveBeenCalled();
    updates.hold(false);
    // Not asked for: saving alone never reloads.
    expect(reload).not.toHaveBeenCalled();
    updates.apply();
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('reloads once the edits are saved when the take-over landed while held', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    await newBuildInstalls();
    updates.apply();
    updates.hold(true);
    container.fire('controllerchange');
    expect(reload).not.toHaveBeenCalled();
    updates.hold(false);
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('asks again once the edits are saved when the take-over was refused for them', async () => {
    const { container, updates, reload, newBuildInstalls, refuse } = setup();
    const worker = await newBuildInstalls();
    updates.apply();
    updates.hold(true);
    refuse();
    expect(updates.state()).toEqual({ ready: true, held: true, blocked: false });
    updates.hold(false);
    expect(worker.messages).toEqual([SKIP, SKIP]);
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('says so when another tab holds unsaved edits, and Reload asks again', async () => {
    const { container, updates, reload, newBuildInstalls, refuse } = setup();
    const worker = await newBuildInstalls();
    updates.apply();
    refuse();
    expect(updates.state()).toEqual({ ready: true, held: false, blocked: true });
    // Saving in this tab is not what was in the way: nothing is retried.
    updates.hold(true);
    updates.hold(false);
    expect(worker.messages).toEqual([SKIP]);
    updates.apply();
    expect(updates.state().blocked).toBe(false);
    expect(worker.messages).toEqual([SKIP, SKIP]);
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('drops the banner when the waiting build goes away', async () => {
    const { updates, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    worker.become('redundant');
    expect(updates.state()).toEqual({ ready: false, held: false, blocked: false });
    updates.apply();
    expect(worker.messages).toEqual([]);
  });

  it('offers the newer build when one replaces the waiting build', async () => {
    const { updates, newBuildInstalls } = setup();
    const first = await newBuildInstalls();
    const second = await newBuildInstalls();
    first.become('redundant');
    expect(updates.state().ready).toBe(true);
    updates.apply();
    expect(first.messages).toEqual([]);
    expect(second.messages).toEqual([SKIP]);
  });

  it('ignores the first install taking over an uncontrolled page', async () => {
    const { container, updates, reload, newBuildInstalls } = setup({ controlled: false });
    await newBuildInstalls();
    container.fire('controllerchange');
    expect(updates.state().ready).toBe(false);
    expect(reload).not.toHaveBeenCalled();
  });

  it("answers the worker's hold question with this tab's unsaved edits", async () => {
    const { updates, askHold } = setup();
    await expect(askHold()).resolves.toEqual({ held: false });
    updates.hold(true);
    await expect(askHold()).resolves.toEqual({ held: true });
  });

  it('stays quiet when the browser refuses to register', async () => {
    const container: UpdateContainer = {
      controller: null,
      register: () => Promise.reject(new Error('blocked')),
      addEventListener: () => {},
    };
    const updates = watchForUpdates(container, vi.fn());
    await Promise.resolve();
    await Promise.resolve();
    expect(updates.state()).toEqual({ ready: false, held: false, blocked: false });
  });
});

describe('windowLoaded', () => {
  it('resolves at once when the window has loaded already', async () => {
    const win = { document: { readyState: 'complete' as DocumentReadyState }, addEventListener: vi.fn() };
    await expect(windowLoaded(win)).resolves.toBeUndefined();
    expect(win.addEventListener).not.toHaveBeenCalled();
  });

  it("waits for the window's load event otherwise", async () => {
    const target = new EventTarget();
    const win = {
      document: { readyState: 'interactive' as DocumentReadyState },
      addEventListener: target.addEventListener.bind(target),
    };
    let done = false;
    const loaded = windowLoaded(win).then(() => (done = true));
    await Promise.resolve();
    expect(done).toBe(false);
    target.dispatchEvent(new Event('load'));
    await loaded;
    expect(done).toBe(true);
  });
});

describe('NO_UPDATES', () => {
  it('never has an update, whatever the page asks', () => {
    NO_UPDATES.hold(true);
    NO_UPDATES.apply();
    expect(NO_UPDATES.state()).toEqual({ ready: false, held: false, blocked: false });
    expect(typeof NO_UPDATES.subscribe(() => {})).toBe('function');
  });
});
