import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from '../../platform/fileHandle';
import { copyName, saveCopy } from './saveCopy';

const bytes = new Uint8Array([37, 80, 68, 70]);

function newFile(options: { failWrite?: boolean } = {}) {
  const log: string[] = [];
  const handle: WritableFileHandle = {
    name: 'chosen.pdf',
    createWritable: async () => ({
      write: async () => {
        if (options.failWrite) throw new Error('disk full');
        log.push('write');
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

describe('copyName', () => {
  it('puts (copy) before the extension', () => {
    expect(copyName('report.pdf')).toBe('report (copy).pdf');
    expect(copyName('Report.PDF')).toBe('Report (copy).pdf');
  });

  it('gives a name without the extension one', () => {
    expect(copyName('report')).toBe('report (copy).pdf');
  });
});

describe('saveCopy', () => {
  it('writes the bytes to the new file the user chose', async () => {
    const { handle, log } = newFile();
    const { calls, download } = downloads();
    expect(await saveCopy({ bytes, name: 'a.pdf', handle, download })).toEqual({ how: 'copied', name: 'chosen.pdf' });
    expect(log).toEqual(['write', 'close']);
    expect(calls).toEqual([]);
  });

  it('downloads the copy under its suggested name where there is no save dialog', async () => {
    const { calls, download } = downloads();
    const outcome = await saveCopy({ bytes, name: 'a.pdf', handle: undefined, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a (copy).pdf', fellBack: null });
    expect(calls).toEqual([[bytes, 'a (copy).pdf']]);
  });

  it('aborts the write and downloads the copy when writing the new file fails', async () => {
    const { handle, log } = newFile({ failWrite: true });
    const { calls, download } = downloads();
    const outcome = await saveCopy({ bytes, name: 'a.pdf', handle, download });
    expect(outcome).toEqual({ how: 'downloaded', name: 'a (copy).pdf', fellBack: 'write' });
    expect(log).toEqual(['abort']);
    expect(calls).toHaveLength(1);
  });
});
