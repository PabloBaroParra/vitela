import { afterEach, describe, expect, it, vi } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
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
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf', fileHandle })).toEqual({ kind: 'copy', handle: undefined });
    expect(picker).not.toHaveBeenCalled();
  });
});
