import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { saveBytes } from './saveBytes';

const bytes = new Uint8Array([37, 80, 68, 70]);

/** A handle that records what is written and when it is committed. */
function fakeHandle(options: { failWrite?: boolean } = {}) {
  const log: string[] = [];
  const handle: WritableFileHandle = {
    name: 'report.pdf',
    createWritable: async () => ({
      write: async (data) => {
        if (options.failWrite) throw new Error('disk full');
        log.push(`write ${[...data].join(',')}`);
      },
      close: async () => void log.push('close'),
      abort: async () => void log.push('abort'),
    }),
  };
  return { handle, log };
}

function downloads() {
  const calls: [Uint8Array, string][] = [];
  return { calls, download: (data: Uint8Array, name: string) => void calls.push([data, name]) };
}

describe('saveBytes', () => {
  it('downloads when there is no file handle', async () => {
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle: undefined, permission: 'granted', download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: null });
    expect(calls).toEqual([[bytes, 'a.pdf']]);
  });

  it('writes over the opened file and commits it', async () => {
    const { handle, log } = fakeHandle();
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, permission: 'granted', download });
    expect(outcome).toEqual({ how: 'wrote', name: 'report.pdf' });
    expect(log).toEqual(['write 37,80,68,70', 'close']);
    expect(calls).toEqual([]);
  });

  it.each(['denied', 'prompt'] as const)('downloads, saying the permission was refused, when it is %s', async (permission) => {
    const { handle, log } = fakeHandle();
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, permission, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: 'permission' });
    expect(log).toEqual([]);
    expect(calls).toHaveLength(1);
  });

  it('aborts the write and downloads instead when the write fails', async () => {
    const { handle, log } = fakeHandle({ failWrite: true });
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, permission: 'granted', download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: 'write' });
    expect(log).toEqual(['abort']);
    expect(calls).toHaveLength(1);
  });

  it('downloads, saying the file could not be checked, and never writes when the destination is unverified', async () => {
    const { handle, log } = fakeHandle();
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, permission: 'granted', unverified: true, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: 'unverified' });
    expect(log).toEqual([]);
    expect(calls).toEqual([[bytes, 'a.pdf']]);
  });
});
