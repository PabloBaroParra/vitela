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

  it('a copy asks the save dialog for "name (copy).pdf" and never looks at the opened file handle', async () => {
    const asked: unknown[] = [];
    win.showSaveFilePicker = async (options: unknown) => (asked.push(options), picked);
    const fileHandle = { name: 'a.pdf' } as WritableFileHandle;
    const destination = await browserDelivery.prepare('copy', { name: 'a.pdf', fileHandle });
    expect(destination).toEqual({ kind: 'copy', handle: picked });
    expect(asked).toMatchObject([{ suggestedName: 'a (copy).pdf' }]);
  });

  it('a copy is cancelled when the user dismisses the save dialog', async () => {
    win.showSaveFilePicker = async () => {
      throw new DOMException('dismissed', 'AbortError');
    };
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf' })).toBeNull();
  });

  it('a copy is a download where there is no save dialog, or it fails', async () => {
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf' })).toEqual({ kind: 'copy', handle: undefined });
    win.showSaveFilePicker = vi.fn(async () => {
      throw new DOMException('blocked', 'SecurityError');
    });
    expect(await browserDelivery.prepare('copy', { name: 'a.pdf' })).toEqual({ kind: 'copy', handle: undefined });
  });
});
