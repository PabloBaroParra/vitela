import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from './fileHandle';
import { adoptCurrentFile, checkFileStamp, recordFileStamp } from './fileStamp';

/** A handle whose file is whatever `disk` says now; `disk.fail` makes getFile throw. */
function diskHandle(disk: { lastModified: number; size: number; fail?: boolean }) {
  const handle: WritableFileHandle = {
    name: 'a.pdf',
    createWritable: async () => ({ write: async () => {}, close: async () => {} }),
    getFile: async () => {
      if (disk.fail) throw new Error('NotFoundError');
      return { lastModified: disk.lastModified, size: disk.size };
    },
  };
  return handle;
}

describe('file stamp', () => {
  it('is unchanged while the file keeps its modified time and size', async () => {
    const disk = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    expect(await checkFileStamp(handle)).toBe('unchanged');
  });

  it('is changed when the modified time moves', async () => {
    const disk = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.lastModified = 2000;
    expect(await checkFileStamp(handle)).toBe('changed');
  });

  it('is changed when only the size moves', async () => {
    const disk = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.size = 51;
    expect(await checkFileStamp(handle)).toBe('changed');
  });

  it('is unknown when the file cannot be read now', async () => {
    const disk: { lastModified: number; size: number; fail?: boolean } = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.fail = true;
    expect(await checkFileStamp(handle)).toBe('unknown');
  });

  it('is unknown when the file could not be read at open, even if it reads now', async () => {
    const disk: { lastModified: number; size: number; fail?: boolean } = { lastModified: 1000, size: 50, fail: true };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.fail = false;
    expect(await checkFileStamp(handle)).toBe('unknown');
  });

  it('is unknown for a handle nothing was recorded for', async () => {
    expect(await checkFileStamp(diskHandle({ lastModified: 1, size: 1 }))).toBe('unknown');
  });

  it('cannot be checked, so is not blocked, on a handle that cannot be read at all', async () => {
    const handle: WritableFileHandle = { name: 'a.pdf', createWritable: async () => ({ write: async () => {}, close: async () => {} }) };
    recordFileStamp(handle);
    expect(await checkFileStamp(handle)).toBe('unchanged');
  });

  it('adopts the file as it is now, so a later save compares against it', async () => {
    const disk = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.lastModified = 2000;
    disk.size = 60;
    await adoptCurrentFile(handle);
    expect(await checkFileStamp(handle)).toBe('unchanged');
  });

  it('adopting a file that cannot be read leaves it unknown', async () => {
    const disk: { lastModified: number; size: number; fail?: boolean } = { lastModified: 1000, size: 50 };
    const handle = diskHandle(disk);
    recordFileStamp(handle);
    disk.fail = true;
    await adoptCurrentFile(handle);
    disk.fail = false;
    expect(await checkFileStamp(handle)).toBe('unknown');
  });
});
