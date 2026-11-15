import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { checkFileStamp, recordFileStamp } from '../../platform/fileStamp';
import { browserDelivery } from './delivery';

const picked = { kind: 'file', name: 'chosen.pdf', createWritable: async () => ({}) } as unknown as WritableFileHandle;
const win = window as unknown as { showSaveFilePicker?: unknown };

afterEach(() => {
  delete win.showSaveFilePicker;
});

describe('browserDelivery.prepare', () => {
  it('asks for write permission on the opened file, before anything else', async () => {
    const log: string[] = [];
    const fileHandle: WritableFileHandle = {
      name: 'a.pdf',
      createWritable: async () => ({ write: async () => {}, close: async () => {} }),
      queryPermission: async () => (log.push('query'), 'prompt'),
      requestPermission: async () => (log.push('request'), 'denied'),
    };
    const destination = await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle });
    expect(destination).toEqual({ kind: 'overwrite', handle: fileHandle, permission: 'denied' });
    expect(log).toEqual(['query', 'request']);
  });

  it('overwrite without a handle is a plain download destination', async () => {
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf' })).toEqual({
      kind: 'overwrite',
      handle: undefined,
      permission: 'granted',
    });
  });

  it('a copy is always a download: no save dialog, so no existing file can be emptied', async () => {
    // Chromium's showSaveFilePicker truncates an existing file it hands back
    // (file_system_access_manager_impl.cc, DidCreateAndTruncateSaveFile), so
    // picking the opened file there would empty it before any check could run.
    const picker = vi.fn(async () => picked);
    win.showSaveFilePicker = picker;
    const fileHandle = { name: 'a.pdf' } as WritableFileHandle;
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'copy' });
    expect(picker).not.toHaveBeenCalled();
  });
});

describe('browserDelivery.prepare: the opened file changed on disk', () => {
  /** A handle onto a file whose disk state the test moves; it logs the permission traffic. */
  function watched(log: string[] = []) {
    const disk: { lastModified: number; size: number; fail?: boolean } = { lastModified: 1000, size: 50 };
    const fileHandle: WritableFileHandle = {
      name: 'a.pdf',
      createWritable: async () => ({ write: async () => {}, close: async () => {} }),
      getFile: async () => {
        if (disk.fail) throw new Error('gone');
        return { lastModified: disk.lastModified, size: disk.size };
      },
      queryPermission: async () => (log.push('query'), 'granted'),
    };
    recordFileStamp(fileHandle);
    return { disk, fileHandle, log };
  }

  it('overwrites as before while the file is as it was opened', async () => {
    const { fileHandle } = watched();
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({
      kind: 'overwrite',
      handle: fileHandle,
      permission: 'granted',
    });
  });

  it('asks instead of writing when the file changed, and does not request permission yet', async () => {
    const { disk, fileHandle, log } = watched();
    disk.lastModified = 2000;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
    expect(log).toEqual([]);
  });

  it('"overwrite anyway" accepts the file as it is now, then goes through the normal permission step', async () => {
    const { disk, fileHandle, log } = watched();
    disk.lastModified = 2000;
    const destination = await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle });
    expect(destination).toEqual({ kind: 'overwrite', handle: fileHandle, permission: 'granted' });
    expect(log).toEqual(['query']);
    expect(await checkFileStamp(fileHandle)).toBe('unchanged');
  });

  it('cannot tell, so does not overwrite: the destination is marked unverified and no permission is asked', async () => {
    const { disk, fileHandle, log } = watched();
    disk.fail = true;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({
      kind: 'overwrite',
      handle: fileHandle,
      permission: 'granted',
      unverified: true,
    });
    expect(log).toEqual([]);
  });

  it('"overwrite anyway" on a file that cannot be read is unverified too', async () => {
    const { disk, fileHandle } = watched();
    disk.fail = true;
    const destination = await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle });
    expect(destination).toMatchObject({ kind: 'overwrite', unverified: true });
  });

  it('a copy never looks at the file', async () => {
    const { disk, fileHandle } = watched();
    disk.lastModified = 2000;
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'copy' });
  });
});
