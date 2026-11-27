import { describe, expect, it, vi } from 'vitest';
import { NO_UPDATES, watchForUpdates, type UpdateContainer, type UpdateRegistration, type UpdateWorker } from './updates';

class FakeTarget {
  private readonly listeners = new Map<string, Array<() => void>>();
  addEventListener(type: string, listener: () => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener]);
  }
  fire(type: string) {
    for (const listener of this.listeners.get(type) ?? []) listener();
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

/** A page's service worker container; `controlled` says whether a worker already serves it. */
function setup({ controlled = true, waiting = false } = {}) {
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
  const updates = watchForUpdates(container, reload);
  /** A new build finishes installing in the background. */
  async function newBuildInstalls() {
    await Promise.resolve();
    const worker = new FakeWorker();
    registration.installing = worker;
    registration.fire('updatefound');
    worker.become('installed');
    return worker;
  }
  return { container, registration, reload, updates, newBuildInstalls };
}

describe('watchForUpdates', () => {
  it('registers /sw.js for the whole site', async () => {
    const { container } = setup();
    await Promise.resolve();
    expect(container.registered).toEqual([['/sw.js', { scope: '/' }]]);
  });

  it('has nothing to offer until a new build is installed', async () => {
    const { updates } = setup();
    await Promise.resolve();
    expect(updates.state()).toEqual({ ready: false, held: false });
  });

  it('offers the update once a new build is installed, and tells subscribers', async () => {
    const { updates, newBuildInstalls } = setup();
    const listener = vi.fn();
    updates.subscribe(listener);
    await newBuildInstalls();
    expect(updates.state()).toEqual({ ready: true, held: false });
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
    expect(worker.messages).toEqual([{ type: 'SKIP_WAITING' }]);
    expect(reload).not.toHaveBeenCalled();
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('never reloads while the open document has unsaved edits', async () => {
    const { container, updates, reload, newBuildInstalls } = setup();
    const worker = await newBuildInstalls();
    updates.hold(true);
    expect(updates.state()).toEqual({ ready: true, held: true });
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
    expect(worker.messages).toEqual([{ type: 'SKIP_WAITING' }]);
    container.fire('controllerchange');
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it('does not reload when another tab activated the new build', async () => {
    const { container, reload, newBuildInstalls } = setup();
    await newBuildInstalls();
    container.fire('controllerchange');
    expect(reload).not.toHaveBeenCalled();
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
    expect(updates.state()).toEqual({ ready: false, held: false });
  });
});

describe('NO_UPDATES', () => {
  it('never has an update, whatever the page asks', () => {
    NO_UPDATES.hold(true);
    NO_UPDATES.apply();
    expect(NO_UPDATES.state()).toEqual({ ready: false, held: false });
    expect(typeof NO_UPDATES.subscribe(() => {})).toBe('function');
  });
});
