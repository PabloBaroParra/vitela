import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from './fileHandle';
import { ensureWritePermission, writeToHandle } from './fileWrite';

const bytes = new Uint8Array([1, 2, 3]);

interface HandleOptions extends Partial<WritableFileHandle> {
  readonly failAt?: 'create' | 'write' | 'close';
  readonly failAbort?: boolean;
}

function handleWith({ failAt, failAbort, ...rest }: HandleOptions) {
  const log: string[] = [];
  const handle: WritableFileHandle = {
    name: 'a.pdf',
    createWritable: async () => {
      if (failAt === 'create') throw new Error('no swap file');
      return {
        write: async () => {
          if (failAt === 'write') throw new Error('disk full');
          log.push('write');
        },
        close: async () => {
          if (failAt === 'close') throw new Error('rename failed');
          log.push('close');
        },
        abort: async () => {
          log.push('abort');
          if (failAbort) throw new Error('already closed');
        },
      };
    },
    ...rest,
  };
  return { handle, log };
}

describe('ensureWritePermission', () => {
  it('is granted without asking when the handle already has it', async () => {
    const asked: string[] = [];
    const { handle } = handleWith({
      queryPermission: async () => 'granted',
      requestPermission: async () => (asked.push('request'), 'granted'),
    });
    expect(await ensureWritePermission(handle)).toBe('granted');
    expect(asked).toEqual([]);
  });

  it('asks when it is not granted yet, and reports the answer', async () => {
    const { handle } = handleWith({ queryPermission: async () => 'prompt', requestPermission: async () => 'denied' });
    expect(await ensureWritePermission(handle)).toBe('denied');
  });

  it('is granted for a handle with no permission API', async () => {
    expect(await ensureWritePermission(handleWith({}).handle)).toBe('granted');
  });

  it('is denied when the permission call throws', async () => {
    const { handle } = handleWith({
      queryPermission: async () => {
        throw new Error('gone');
      },
    });
    expect(await ensureWritePermission(handle)).toBe('denied');
  });
});

describe('writeToHandle', () => {
  it('writes and commits', async () => {
    const { handle, log } = handleWith({});
    expect(await writeToHandle(handle, bytes)).toBe(true);
    expect(log).toEqual(['write', 'close']);
  });

  it.each(['write', 'close'] as const)('aborts the writable when %s fails, so no half-file is committed', async (failAt) => {
    const { handle, log } = handleWith({ failAt });
    expect(await writeToHandle(handle, bytes)).toBe(false);
    expect(log.at(-1)).toBe('abort');
  });

  it('still reports the failure when abort itself throws', async () => {
    const { handle } = handleWith({ failAt: 'write', failAbort: true });
    expect(await writeToHandle(handle, bytes)).toBe(false);
  });

  it('fails without a writable to abort when it cannot be created', async () => {
    const { handle, log } = handleWith({ failAt: 'create' });
    expect(await writeToHandle(handle, bytes)).toBe(false);
    expect(log).toEqual([]);
  });
});
