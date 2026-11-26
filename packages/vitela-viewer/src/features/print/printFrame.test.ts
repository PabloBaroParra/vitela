import { describe, expect, it } from 'vitest';
import { decodeInOrder } from './printFrame';

/** Images whose decoding the test lets go one by one, counting how many decode at once. */
function images(count: number, failing?: number) {
  const releases: (() => void)[] = [];
  let decoding = 0;
  let most = 0;
  const decoded: number[] = [];
  const list = Array.from({ length: count }, (_, i) => ({
    decode: () => {
      decoding++;
      most = Math.max(most, decoding);
      return new Promise<void>((resolve, reject) =>
        releases.push(() => {
          decoding--;
          decoded.push(i);
          if (i === failing) reject(new Error('broken'));
          else resolve();
        }),
      );
    },
  }));
  return { list, releases, decoded, most: () => most };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('decodeInOrder', () => {
  it('decodes one page at a time, in order, and a page that fails does not stop the rest', async () => {
    const { list, releases, decoded, most } = images(3, 1);
    const done = decodeInOrder(list, () => false);
    for (let i = 0; i < 3; i++) {
      await settle();
      expect(releases).toHaveLength(i + 1);
      releases[i]!();
    }
    expect(await done).toBe(true);
    expect(decoded).toEqual([0, 1, 2]);
    expect(most()).toBe(1);
  });

  it('stops decoding once the print is cancelled', async () => {
    const { list, releases } = images(3);
    let stopped = false;
    const done = decodeInOrder(list, () => stopped);
    await settle();
    stopped = true;
    releases[0]!();
    expect(await done).toBe(false);
    expect(releases).toHaveLength(1);
  });
});
