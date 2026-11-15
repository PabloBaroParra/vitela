import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from './fileHandle';
import { saveBytes } from './saveBytes';

const bytes = new Uint8Array([37, 80, 68, 70]);

/** A handle that records what is written and when it is committed. */
function fakeHandle(options: { permission?: PermissionState; granted?: PermissionState; failWrite?: boolean } = {}) {
  const log: string[] = [];
  const handle: WritableFileHandle = {
    name: 'report.pdf',
    queryPermission: async () => options.permission ?? 'granted',
    requestPermission: async () => {
      log.push('request');
      return options.granted ?? 'granted';
    },
    createWritable: async () => ({
      write: async (data) => {
        if (options.failWrite) throw new Error('disk full');
        log.push(`write ${[...data].join(',')}`);
      },
      close: async () => void log.push('close'),
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
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle: undefined, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: false });
    expect(calls).toEqual([[bytes, 'a.pdf']]);
  });

  it('writes over the opened file and commits it', async () => {
    const { handle, log } = fakeHandle();
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, download });
    expect(outcome).toEqual({ how: 'wrote', name: 'report.pdf' });
    expect(log).toEqual(['write 37,80,68,70', 'close']);
    expect(calls).toEqual([]);
  });

  it('asks for write permission when the handle has not been granted it', async () => {
    const { handle, log } = fakeHandle({ permission: 'prompt' });
    const { download } = downloads();
    expect(await saveBytes({ bytes, name: 'a.pdf', handle, download })).toMatchObject({ how: 'wrote' });
    expect(log[0]).toBe('request');
  });

  it('downloads instead, and says so, when write permission is refused', async () => {
    const { handle, log } = fakeHandle({ permission: 'prompt', granted: 'denied' });
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: true });
    expect(log).toEqual(['request']);
    expect(calls).toHaveLength(1);
  });

  it('downloads instead when the write fails', async () => {
    const { handle } = fakeHandle({ failWrite: true });
    const { calls, download } = downloads();
    const outcome = await saveBytes({ bytes, name: 'a.pdf', handle, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a.pdf', fellBack: true });
    expect(calls).toHaveLength(1);
  });
});
