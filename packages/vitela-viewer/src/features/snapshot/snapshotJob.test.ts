import { describe, expect, it } from 'vitest';
import { EngineError, type Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { PageNotDrawn, drawPages, withSnapshot, type JobGuard } from './snapshotJob';

function guarded(state: { current?: boolean; stopped?: boolean } = {}): JobGuard & { state: { current: boolean; stopped: boolean } } {
  const s = { current: state.current ?? true, stopped: state.stopped ?? false };
  return { state: s, current: () => s.current, stopped: () => s.stopped };
}

function snapshotEngine(log: string[], overrides: Partial<Engine> = {}): Engine {
  return fakeEngine({
    exportPageImage: async (snapshot, page, dpi, format) => (log.push(`draw:${snapshot}:${page}:${dpi}:${format}`), new Uint8Array([page])),
    closeImageExport: async (snapshot) => (log.push(`close:${snapshot}`), true),
    ...overrides,
  });
}

const opening = (log: string[]) => async () => (log.push('open'), { handle: 40, pageCount: 3 });

describe('withSnapshot', () => {
  it('settles the edits, opens the snapshot, runs the job on it and closes it', async () => {
    const log: string[] = [];
    const engine = snapshotEngine(log);
    const result = await withSnapshot(engine, guarded(), async () => void log.push('settled'), opening(log), async (snapshot) => {
      log.push(`job:${snapshot.handle}:${snapshot.pageCount}`);
      return 'drawn';
    });
    expect(result).toBe('drawn');
    expect(log).toEqual(['settled', 'open', 'job:40:3', 'close:40']);
  });

  it('closes the snapshot when the job fails, and the failure reaches the caller', async () => {
    const log: string[] = [];
    const engine = snapshotEngine(log);
    const failing = withSnapshot(engine, guarded(), undefined, opening(log), async () => {
      throw new Error('no');
    });
    await expect(failing).rejects.toThrow('no');
    expect(log).toEqual(['open', 'close:40']);
  });

  it('opens nothing for a job dropped while the edits settled, and runs nothing on one dropped while it opened', async () => {
    const log: string[] = [];
    const engine = snapshotEngine(log);
    const guard = guarded();
    const settle = async () => void (guard.state.current = false);
    expect(await withSnapshot(engine, guard, settle, opening(log), async () => log.push('job'))).toBeUndefined();
    expect(log).toEqual([]);

    const later = guarded();
    const open = async () => ((later.state.current = false), log.push('open'), { handle: 41, pageCount: 1 });
    expect(await withSnapshot(engine, later, undefined, open, async () => log.push('job'))).toBeUndefined();
    expect(log).toEqual(['open', 'close:41']);
  });

  it('still settles when closing the snapshot fails', async () => {
    const engine = fakeEngine({ closeImageExport: async () => Promise.reject(new Error('gone')) });
    expect(await withSnapshot(engine, guarded(), undefined, async () => ({ handle: 1, pageCount: 1 }), async () => 'ok')).toBe('ok');
  });
});

describe('drawPages', () => {
  it('draws the pages one at a time, in the order asked, handing each over before the next', async () => {
    const log: string[] = [];
    const engine = snapshotEngine(log);
    const end = await drawPages(engine, 40, [2, 0], 96, 'jpeg', guarded(), {
      starting: (page, index) => void log.push(`start:${page}:${index}`),
      drawn: async (bytes, page, index) => void log.push(`took:${page}:${index}:${bytes[0]}`),
    });
    expect(end).toBe('finished');
    expect(log).toEqual(['start:2:0', 'draw:40:2:96:jpeg', 'took:2:0:2', 'start:0:1', 'draw:40:0:96:jpeg', 'took:0:1:0']);
  });

  it('stops before the next page once asked, after the page in hand went out', async () => {
    const log: string[] = [];
    const engine = snapshotEngine(log);
    const guard = guarded();
    const end = await drawPages(engine, 40, [0, 1, 2], 150, 'png', guard, {
      starting: () => {},
      drawn: (_, page) => {
        log.push(`took:${page}`);
        guard.state.stopped = true;
      },
    });
    expect(end).toBe('stopped');
    expect(log).toEqual(['draw:40:0:150:png', 'took:0']);
  });

  it('hands over nothing drawn for a job dropped meanwhile', async () => {
    const log: string[] = [];
    const guard = guarded();
    const engine = snapshotEngine(log, {
      exportPageImage: async () => ((guard.state.current = false), new Uint8Array()),
    });
    const end = await drawPages(engine, 40, [0, 1], 150, 'png', guard, { starting: () => {}, drawn: () => void log.push('took') });
    expect(end).toBe('dropped');
    expect(log).toEqual([]);
  });

  it("names the page the engine could not draw, keeping the engine's error", async () => {
    const cause = new EngineError('RenderFailed', 'the page could not be rendered');
    const engine = snapshotEngine([], {
      exportPageImage: async (_, page) => {
        if (page === 1) throw cause;
        return new Uint8Array();
      },
    });
    const drawing = drawPages(engine, 40, [0, 1], 150, 'png', guarded(), { starting: () => {}, drawn: () => {} });
    const error = await drawing.catch((e: unknown) => e);
    expect(error).toBeInstanceOf(PageNotDrawn);
    expect(error).toMatchObject({ pageIndex: 1, cause });
  });

  it('lets a failure taking a page through as it is', async () => {
    const engine = snapshotEngine([]);
    const failing = drawPages(engine, 40, [0], 150, 'png', guarded(), {
      starting: () => {},
      drawn: () => {
        throw new Error('not written');
      },
    });
    await expect(failing).rejects.toThrow('not written');
  });
});
