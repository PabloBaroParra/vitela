import { describe, expect, it } from 'vitest';
import { callbackDelivery } from './callbackDelivery';
import type { SaveIntent } from './delivery';

const bytes = new Uint8Array([37, 80, 68, 70]);

describe('callbackDelivery', () => {
  it('settles every save straight away: nothing to ask the browser, no file to check', async () => {
    const delivery = callbackDelivery(() => {});
    for (const intent of ['overwrite', 'overwriteAnyway', 'copy'] as SaveIntent[]) {
      const destination = await delivery.prepare(intent, { name: 'a.pdf' });
      expect(destination).not.toBeNull();
      expect(destination?.kind).not.toBe('changedOnDisk');
    }
  });

  it('hands the bytes and the document name to the callback, and reports them delivered', async () => {
    const calls: [Uint8Array, string][] = [];
    const delivery = callbackDelivery((saved, name) => void calls.push([saved, name]));
    const destination = await delivery.prepare('overwrite', { name: 'a.pdf' });

    await expect(delivery.deliver({ bytes, name: 'a.pdf', destination: destination as never })).resolves.toEqual({
      how: 'delivered',
      name: 'a.pdf',
    });
    expect(calls).toEqual([[bytes, 'a.pdf']]);
  });

  it('tells the callback what kind of file it is: a PDF unless the request says otherwise', async () => {
    const calls: [string, string][] = [];
    const delivery = callbackDelivery((_saved, name, type) => void calls.push([name, type]));
    await delivery.deliver({ bytes, name: 'a.pdf', destination: { kind: 'copy' } });
    await delivery.deliver({ bytes, name: 'a-1.png', type: 'image/png', destination: { kind: 'newFile' } });
    expect(calls).toEqual([
      ['a.pdf', 'application/pdf'],
      ['a-1.png', 'image/png'],
    ]);
  });

  it('waits for an asynchronous callback, and a rejection fails the save', async () => {
    let finish!: () => void;
    const delivery = callbackDelivery(() => new Promise<void>((resolve) => (finish = resolve)));
    let settled = false;
    const delivered = delivery.deliver({ bytes, name: 'a.pdf', destination: { kind: 'copy' } }).then(() => (settled = true));
    await Promise.resolve();
    expect(settled).toBe(false);
    finish();
    await delivered;
    expect(settled).toBe(true);

    const failing = callbackDelivery(() => Promise.reject(new Error('host storage is full')));
    await expect(failing.deliver({ bytes, name: 'a.pdf', destination: { kind: 'copy' } })).rejects.toThrow(
      'host storage is full',
    );
  });
});
