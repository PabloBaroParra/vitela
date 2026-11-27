// PDFs the OS opens with the installed app (T-266).
//
// The manifest's `file_handlers` offer the installed app for PDFs ("Open
// with", double-click once it is the default); Chromium then starts it and
// hands the files over through `window.launchQueue`, as FileSystemFileHandles.
// Other browsers have no launch queue and install no file handlers: there is
// nothing to watch.
//
// The consumer is set once, at startup, and keeps the newest launched file
// until the page can open it (the engine loads after the launch arrives). A
// launched file opens as a picked one does: the same open, with the handle so
// Save writes back over it. Several files launched together open as several
// dropped ones do: the first PDF, else the first file (the engine then says
// why it is not a PDF).
//
// The manifest's `launch_handler` gives every launch a new window, so a
// launched file never replaces a document with unsaved edits in another
// window; and the window it would replace in place has none to lose.

/** The handle Chromium launches the app with: a `FileSystemFileHandle`. Structural so tests can fake it. */
export interface LaunchFileHandle {
  readonly kind: string;
  readonly name: string;
  getFile(): Promise<File>;
  /** Present on a file the app may write back to (Save then saves over it). */
  createWritable?: () => Promise<{ write(data: Uint8Array): Promise<void>; close(): Promise<void> }>;
}

/** The launch's parameters; `files` is empty when the app was started without a file. */
export interface LaunchParams {
  readonly files: readonly LaunchFileHandle[];
}
export type LaunchConsumer = (params: LaunchParams) => void;
export interface LaunchQueue {
  setConsumer(consumer: LaunchConsumer): void;
}

/** A launched file, ready to open; `handle` when the browser lets Save write back to it. */
export interface LaunchedFile {
  readonly file: File;
  readonly handle?: LaunchFileHandle & { readonly createWritable: NonNullable<LaunchFileHandle['createWritable']> };
}

/** What the page reads of the launches (`useSyncExternalStore`-shaped). */
export interface AppLaunches {
  subscribe(listener: () => void): () => void;
  /** The launched file the page has not opened yet; `null` when there is none. */
  pending(): LaunchedFile | null;
  /** Hands `launched` over: `true` the first time, so the page opens it once. */
  take(launched: LaunchedFile): boolean;
}

/** For browsers without a launch queue, dev and tests: nothing is ever launched. */
export const NO_LAUNCHES: AppLaunches = {
  subscribe: () => () => {},
  pending: () => null,
  take: () => false,
};

/** The browser's launch queue, where it has one (Chromium). */
export function launchQueueOf(win: object): LaunchQueue | undefined {
  const queue = (win as { launchQueue?: Partial<LaunchQueue> }).launchQueue;
  return typeof queue?.setConsumer === 'function' ? (queue as LaunchQueue) : undefined;
}

const isPdf = (file: File): boolean => file.type === 'application/pdf' || file.name.toLowerCase().endsWith('.pdf');

const writable = (handle: LaunchFileHandle): handle is NonNullable<LaunchedFile['handle']> =>
  typeof handle.createWritable === 'function';

/** Each file handle's file, or `null` where it cannot be read. */
function readAll(handles: readonly LaunchFileHandle[]): Promise<(LaunchedFile | null)[]> {
  return Promise.all(
    handles.map(async (handle) => {
      if (handle.kind !== 'file') return null;
      try {
        const file = await handle.getFile();
        return writable(handle) ? { file, handle } : { file };
      } catch {
        return null;
      }
    }),
  );
}

/** Takes the app's launches from `queue`, from the start. */
export function watchLaunches(queue: LaunchQueue): AppLaunches {
  const listeners = new Set<() => void>();
  let pending: LaunchedFile | null = null;
  /** Counts launches, so one whose files arrive late gives way to a newer one. */
  let latest = 0;

  queue.setConsumer(({ files }) => {
    if (files.length === 0) return;
    const launch = ++latest;
    void readAll(files).then((read) => {
      if (launch !== latest) return;
      const launched = read.filter((entry): entry is LaunchedFile => entry !== null);
      const chosen = launched.find((entry) => isPdf(entry.file)) ?? launched[0];
      if (!chosen) return;
      pending = chosen;
      for (const listener of listeners) listener();
    });
  });

  return {
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    pending: () => pending,
    take(launched) {
      if (pending !== launched) return false;
      pending = null;
      for (const listener of listeners) listener();
      return true;
    },
  };
}
