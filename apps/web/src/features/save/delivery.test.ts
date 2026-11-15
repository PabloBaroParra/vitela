import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { recordFileStamp } from '../../platform/fileStamp';
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
      seen: { lastModified: 1000, size: 50 },
    });
  });

  it('asks for the write permission BEFORE reading the file, while the click is fresh', async () => {
    const log: string[] = [];
    const { fileHandle } = watched(log);
    const getFile = fileHandle.getFile!;
    fileHandle.getFile = async () => (log.push('getFile'), getFile());
    await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle });
    expect(log[0]).toBe('query');
    expect(log.slice(1).every((entry) => entry === 'getFile')).toBe(true);
  });

  it('asks instead of writing when the file changed', async () => {
    const { disk, fileHandle } = watched();
    disk.lastModified = 2000;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
  });

  it('"overwrite anyway" does not adopt the changed file: only a successful write does', async () => {
    const { disk, fileHandle, log } = watched();
    disk.lastModified = 2000;
    const destination = await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle });
    expect(destination).toMatchObject({ kind: 'overwrite', handle: fileHandle, permission: 'granted' });
    expect(log).toEqual(['query']);
    // Nothing was written: the next plain Save must still ask.
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
  });

  it('a refused permission on "overwrite anyway" leaves the change unaccepted', async () => {
    const { disk, fileHandle } = watched();
    fileHandle.queryPermission = async () => 'denied';
    fileHandle.requestPermission = async () => 'denied';
    disk.lastModified = 2000;
    await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle });
    fileHandle.queryPermission = async () => 'granted';
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
  });

  it('cannot tell, so does not overwrite: the destination says it can only be downloaded', async () => {
    const { disk, fileHandle } = watched();
    disk.fail = true;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'unverified' });
  });

  it('"overwrite anyway" on a file that cannot be read is unverified too', async () => {
    const { disk, fileHandle } = watched();
    disk.fail = true;
    expect(await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'unverified' });
  });

  it('a read that failed once does not make every later save a download', async () => {
    const { disk, fileHandle } = watched();
    disk.fail = true;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'unverified' });
    disk.fail = false;
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toMatchObject({ kind: 'overwrite' });
  });

  it('a copy never looks at the file', async () => {
    const { disk, fileHandle } = watched();
    disk.lastModified = 2000;
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'copy' });
  });
});

describe('browserDelivery.deliver: the file changes while the engine works', () => {
  const bytes = new Uint8Array([37, 80]);

  function watched() {
    const disk = { lastModified: 1000, size: 50 };
    const writes: string[] = [];
    const fileHandle: WritableFileHandle = {
      name: 'a.pdf',
      createWritable: async () => ({
        write: async () => void writes.push('write'),
        close: async () => void writes.push('close'),
      }),
      getFile: async () => ({ lastModified: disk.lastModified, size: disk.size }),
      queryPermission: async () => 'granted',
    };
    recordFileStamp(fileHandle);
    return { disk, fileHandle, writes };
  }

  it('writes nothing and asks again when the file moved after prepare', async () => {
    const { disk, fileHandle, writes } = watched();
    const destination = (await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle }))!;
    disk.lastModified = 2000;
    const result = await browserDelivery.deliver({ bytes, name: 'a.pdf', destination: destination as never });
    expect(result).toEqual({ how: 'changedOnDisk' });
    expect(writes).toEqual([]);
  });

  it('writes when the file is as prepare saw it, and the write becomes the new baseline', async () => {
    const { disk, fileHandle, writes } = watched();
    const destination = (await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle }))!;
    const result = await browserDelivery.deliver({ bytes, name: 'a.pdf', destination: destination as never });
    expect(result).toEqual({ how: 'wrote', name: 'a.pdf' });
    expect(writes).toEqual(['write', 'close']);
    disk.lastModified = 3000; // somebody else, after our write
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
  });

  it('a failed write after "overwrite anyway" leaves the change unaccepted', async () => {
    const { disk, fileHandle } = watched();
    disk.lastModified = 2000;
    fileHandle.createWritable = async () => {
      throw new Error('disk full');
    };
    const destination = (await browserDelivery.prepare('overwriteAnyway', { name: 'a.pdf', fileHandle }))!;
    await browserDelivery.deliver({ bytes, name: 'a.pdf', destination: destination as never });
    expect(await browserDelivery.prepare('overwrite', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'changedOnDisk' });
  });
});

describe('browserDelivery.prepare: carrying an accepted change', () => {
  function disked() {
    const disk = { lastModified: 1000, size: 50 };
    const fileHandle: WritableFileHandle = {
      name: 'a.pdf',
      createWritable: async () => ({ write: async () => {}, close: async () => {} }),
      getFile: async () => ({ lastModified: disk.lastModified, size: disk.size }),
      queryPermission: async () => 'granted',
    };
    recordFileStamp(fileHandle);
    return { disk, fileHandle };
  }
  const source = (fileHandle: WritableFileHandle) => ({ name: 'a.pdf', fileHandle });

  it('compares against the accepted stamp, not the opened baseline', async () => {
    const { disk, fileHandle } = disked();
    disk.lastModified = 2000;
    const first = (await browserDelivery.prepare('overwriteAnyway', source(fileHandle))) as unknown as { seen: never };
    const retry = await browserDelivery.prepare('overwrite', source(fileHandle), first.seen);
    expect(retry).toEqual({ kind: 'overwrite', handle: fileHandle, permission: 'granted', seen: first.seen });
  });

  it('asks again when the file changed once more after it was accepted', async () => {
    const { disk, fileHandle } = disked();
    disk.lastModified = 2000;
    const first = (await browserDelivery.prepare('overwriteAnyway', source(fileHandle))) as unknown as { seen: never };
    disk.lastModified = 3000;
    expect(await browserDelivery.prepare('overwrite', source(fileHandle), first.seen)).toEqual({ kind: 'changedOnDisk' });
  });

  it('a stamp that cannot be read before the write means unverified, never a write', async () => {
    const { fileHandle } = disked();
    const real = fileHandle.getFile!;
    let calls = 0;
    fileHandle.getFile = async () => {
      if (++calls >= 2) throw new Error('gone');
      return real();
    };
    expect(await browserDelivery.prepare('overwrite', source(fileHandle))).toEqual({ kind: 'unverified' });
  });
});
