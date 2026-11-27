import { describe, expect, it, vi } from 'vitest';
import { launchQueueOf, watchLaunches, type LaunchConsumer, type LaunchFileHandle, type LaunchQueue } from './launchQueue';

/** A launchQueue that hands its consumer whatever the test launches. */
function fakeQueue() {
  let consumer: LaunchConsumer | undefined;
  const queue: LaunchQueue = { setConsumer: vi.fn((next: LaunchConsumer) => void (consumer = next)) };
  return {
    queue,
    launch(files: readonly LaunchFileHandle[]) {
      if (!consumer) throw new Error('no consumer');
      consumer({ files });
    },
  };
}

/** A FileSystemFileHandle as Chromium launches one: a file it can write back to. */
function fileHandle(name: string, options: { writable?: boolean; type?: string; fails?: boolean } = {}): LaunchFileHandle {
  const { writable = true, type = name.endsWith('.pdf') ? 'application/pdf' : 'text/plain', fails = false } = options;
  const handle: LaunchFileHandle = {
    kind: 'file',
    name,
    getFile: async () => {
      if (fails) throw new DOMException('gone', 'NotFoundError');
      return new File([new Uint8Array([37, 80, 68, 70])], name, { type });
    },
  };
  return writable ? { ...handle, createWritable: async () => ({ write: async () => {}, close: async () => {} }) } : handle;
}

/** Waits until the launch's handles are read. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('launchQueueOf', () => {
  it('finds the launch queue only where the browser has one (Chromium)', () => {
    expect(launchQueueOf({})).toBeUndefined();
    expect(launchQueueOf({ launchQueue: {} })).toBeUndefined();
    const queue: LaunchQueue = { setConsumer: () => {} };
    expect(launchQueueOf({ launchQueue: queue })).toBe(queue);
  });
});

describe('watchLaunches', () => {
  it('takes the launches once, from the start', () => {
    const { queue } = fakeQueue();
    watchLaunches(queue);
    expect(queue.setConsumer).toHaveBeenCalledTimes(1);
  });

  it('has nothing to open when the app was started without a file', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([]);
    await settle();
    expect(launches.pending()).toBeNull();
  });

  it('offers a launched PDF with its handle, as the picker does, and tells the page', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    const listener = vi.fn();
    launches.subscribe(listener);
    const handle = fileHandle('report.pdf');
    launch([handle]);
    await settle();

    const launched = launches.pending();
    expect(launched?.file.name).toBe('report.pdf');
    expect(launched?.handle).toBe(handle);
    expect(listener).toHaveBeenCalledTimes(1);
  });

  it('offers a handle it cannot write back to without one: Save then downloads', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([fileHandle('read-only.pdf', { writable: false })]);
    await settle();
    expect(launches.pending()?.file.name).toBe('read-only.pdf');
    expect(launches.pending()?.handle).toBeUndefined();
  });

  it('opens the first PDF of several, with its own handle, as a drop of several files does', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    const second = fileHandle('second.pdf');
    launch([fileHandle('notes.txt'), second, fileHandle('third.pdf')]);
    await settle();
    expect(launches.pending()?.file.name).toBe('second.pdf');
    expect(launches.pending()?.handle).toBe(second);
  });

  it('opens the first file when none is a PDF, so the engine says why it cannot', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([fileHandle('notes.txt'), fileHandle('more.txt')]);
    await settle();
    expect(launches.pending()?.file.name).toBe('notes.txt');
  });

  it('skips a launched file it cannot read', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([fileHandle('gone.pdf', { fails: true }), fileHandle('here.pdf')]);
    await settle();
    expect(launches.pending()?.file.name).toBe('here.pdf');

    launch([fileHandle('gone.pdf', { fails: true })]);
    await settle();
    // Nothing readable: the file already offered stays.
    expect(launches.pending()?.file.name).toBe('here.pdf');
  });

  it('ignores a launched directory', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([{ ...fileHandle('folder.pdf'), kind: 'directory' }]);
    await settle();
    expect(launches.pending()).toBeNull();
  });

  it('hands each launched file over once', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    launch([fileHandle('once.pdf')]);
    await settle();
    const launched = launches.pending()!;

    expect(launches.take(launched)).toBe(true);
    expect(launches.pending()).toBeNull();
    // A second effect run (StrictMode) must not open it again.
    expect(launches.take(launched)).toBe(false);
  });

  it('keeps only the newest launch, even when an older one is still being read', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    let release!: () => void;
    const slow: LaunchFileHandle = {
      ...fileHandle('older.pdf'),
      getFile: () =>
        new Promise((resolve) => (release = () => resolve(new File([], 'older.pdf', { type: 'application/pdf' })))),
    };
    launch([slow]);
    launch([fileHandle('newer.pdf')]);
    await settle();
    release();
    await settle();
    expect(launches.pending()?.file.name).toBe('newer.pdf');
  });

  it('stops telling a listener that unsubscribed', async () => {
    const { queue, launch } = fakeQueue();
    const launches = watchLaunches(queue);
    const listener = vi.fn();
    launches.subscribe(listener)();
    launch([fileHandle('quiet.pdf')]);
    await settle();
    expect(listener).not.toHaveBeenCalled();
  });
});
