import { describe, expect, it } from 'vitest';
import type { WritableFileHandle } from './fileHandle';
import { canPickSaveTarget, pickSaveTarget } from './saveTarget';

const handle = { kind: 'file', name: 'x (copy).pdf', createWritable: async () => ({}) } as unknown as WritableFileHandle;

describe('pickSaveTarget', () => {
  it('is unavailable without showSaveFilePicker', () => {
    expect(canPickSaveTarget({})).toBe(false);
    expect(canPickSaveTarget({ showSaveFilePicker: async () => handle })).toBe(true);
  });

  it('asks for a PDF with the suggested name and returns the chosen handle', async () => {
    const asked: unknown[] = [];
    const win = { showSaveFilePicker: async (options: unknown) => (asked.push(options), handle) };
    expect(await pickSaveTarget('x (copy).pdf', win)).toBe(handle);
    expect(asked).toEqual([
      { suggestedName: 'x (copy).pdf', types: [{ description: 'PDF', accept: { 'application/pdf': ['.pdf'] } }] },
    ]);
  });

  it('is null when the user dismisses the picker', async () => {
    const win = {
      showSaveFilePicker: async () => {
        throw new DOMException('dismissed', 'AbortError');
      },
    };
    expect(await pickSaveTarget('x.pdf', win)).toBeNull();
  });

  it('lets any other failure through', async () => {
    const win = {
      showSaveFilePicker: async () => {
        throw new DOMException('blocked', 'SecurityError');
      },
    };
    await expect(pickSaveTarget('x.pdf', win)).rejects.toThrow('blocked');
  });
});
