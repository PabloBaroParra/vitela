// The slice of the File System Access API the app uses. Only Chromium has it,
// so everything here is optional: no handle means "download instead".
//
// A handle is obtained where the user hands over a file: `showOpenFilePicker`
// (the Open button) or `DataTransferItem.getAsFileSystemHandle` (a drop).

/** A file the browser lets us write back to. Structural so tests can fake it. */
export interface WritableFileHandle {
  readonly name: string;
  createWritable(): Promise<{
    write(data: Uint8Array): Promise<void>;
    close(): Promise<void>;
    /** Drops the swap file without replacing the original. */
    abort?(reason?: unknown): Promise<void>;
  }>;
  queryPermission?(descriptor: { mode: 'readwrite' }): Promise<PermissionState>;
  requestPermission?(descriptor: { mode: 'readwrite' }): Promise<PermissionState>;
}

/** A file picked together with the handle that can save over it. */
export interface PickedFile {
  readonly file: File;
  readonly handle: WritableFileHandle;
}

interface OpenPickerWindow {
  showOpenFilePicker?: (options: unknown) => Promise<(WritableFileHandle & { getFile(): Promise<File>; kind: string })[]>;
}

interface HandleItem {
  readonly kind: string;
  getAsFileSystemHandle?: () => Promise<{ kind: string } | null>;
}

const isWritable = (handle: { kind: string } | null | undefined): handle is WritableFileHandle & { kind: 'file' } =>
  handle?.kind === 'file' && typeof (handle as Partial<WritableFileHandle>).createWritable === 'function';

/** Whether this browser can open a file together with a handle to save over. */
export function canPickWithHandle(win: object = window): boolean {
  return typeof (win as OpenPickerWindow).showOpenFilePicker === 'function';
}

/** Shows the browser's PDF picker; `null` when the user dismisses it. */
export async function pickPdfWithHandle(win: object = window): Promise<PickedFile | null> {
  const picker = (win as OpenPickerWindow).showOpenFilePicker;
  if (!picker) return null;
  let handles;
  try {
    handles = await picker.call(win, {
      multiple: false,
      types: [{ description: 'PDF', accept: { 'application/pdf': ['.pdf'] } }],
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === 'AbortError') return null;
    throw error;
  }
  const handle = handles[0];
  return handle ? { file: await handle.getFile(), handle } : null;
}

/**
 * The handle of each file item of a drop, started while the drop event is
 * still being dispatched (the browser invalidates the items afterwards). One
 * promise per `dataTransfer.files` entry, in order; a promise resolves to
 * `null` where the browser gives no writable handle. `undefined` when this
 * browser cannot give handles at all.
 */
export function handlesOfDrop(items: Iterable<HandleItem>): Promise<WritableFileHandle | null>[] | undefined {
  const files = [...items].filter((item) => item.kind === 'file');
  if (!files.some((item) => typeof item.getAsFileSystemHandle === 'function')) return undefined;
  return files.map(async (item) => {
    try {
      const handle = (await item.getAsFileSystemHandle?.()) ?? null;
      return isWritable(handle) ? handle : null;
    } catch {
      return null;
    }
  });
}
