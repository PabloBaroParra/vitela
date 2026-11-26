import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError, type Engine, type SplitPart } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { DeliverRequest, Delivery } from '../save/delivery';
import { ExtractControls } from './ExtractControls';
import { SplitControls } from './SplitControls';
import { useExtract } from './useExtract';
import { useSplit } from './useSplit';

function openDocument(pageCount: number): OpenDocument {
  return {
    name: 'a.pdf',
    handle: 7,
    generation: 1,
    info: {
      pageCount,
      pages: Array.from({ length: pageCount }, () => ({ widthPt: 612, heightPt: 792, rotationDegrees: 0 })),
      pageIds: Array.from({ length: pageCount }, (_, i) => i),
      textExtractionAllowed: true,
      documentAssemblyAllowed: true,
      blocks: [],
      importRefusal: null,
      annotationEditingAllowed: true,
      formFillAllowed: true,
    },
  };
}

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

function recording(failOn?: string) {
  const delivered: DeliverRequest[] = [];
  const delivery: Delivery = {
    prepare: async () => {
      throw new Error('a new file needs no destination prepared');
    },
    deliver: async (request) => {
      if (request.name === failOn) throw new Error('blocked');
      delivered.push(request);
      return { how: 'downloaded', name: request.name, fellBack: null };
    },
  };
  return { delivery, delivered };
}

interface EngineOptions {
  readonly refusal?: string | null;
  readonly signed?: boolean;
  /** A part (by its first page) whose extraction fails. */
  readonly failAt?: number;
}

/** The core's grammar for the two shapes these tests type, and its split names. */
function extractEngine({ refusal = null, signed = false, failAt }: EngineOptions = {}) {
  const asked: string[] = [];
  const extracted: number[][] = [];
  const engine = fakeEngine({
    extractionRefusal: async (handle) => (asked.push(`refusal:${handle}`), refusal),
    extractSourceIsSigned: async (handle) => (asked.push(`signed:${handle}`), signed),
    parsePageSelection: async (input, total) => {
      const match = /^(\d+)(?:-(\d+))?$/.exec(input.trim());
      const from = Number(match?.[1]);
      const to = Number(match?.[2] ?? match?.[1]);
      if (!match || to > total) throw new EngineError('InvalidPageSelection', `page ${to} does not exist`);
      return Array.from({ length: to - from + 1 }, (_, i) => from - 1 + i);
    },
    extractPages: async (_handle, pages) => {
      extracted.push([...pages]);
      if (pages[0] === failAt) throw new EngineError('SaveFailed', 'the writer broke');
      return new Uint8Array([37, ...pages]);
    },
    planSplit: async (cuts, total, name) => {
      if (total < 2) throw new EngineError('InvalidPageSelection', 'A document of one page cannot be split.');
      const at = cuts
        .split(',')
        .map((cut) => Number(cut.trim()))
        .filter((cut) => cut > 0);
      if (at.length === 0 || at.some((cut) => cut >= total)) {
        throw new EngineError('InvalidPageSelection', `There is nothing after page ${total}, so the document cannot be split there.`);
      }
      const stem = name.replace(/\.pdf$/, '');
      const parts: SplitPart[] = [];
      let first = 0;
      for (const cut of [...at, total]) {
        parts.push({ first, last: cut - 1, fileName: `${stem}-part${parts.length + 1}.pdf` });
        first = cut;
      }
      return parts;
    },
  });
  return { engine, asked, extracted };
}

function ExtractHarness({ engine, delivery, pages = 5, settle }: { engine: Engine; delivery: Delivery; pages?: number; settle?: () => Promise<unknown> }) {
  const session = useExtract(engine, openDocument(pages), delivery, settle);
  return (
    <>
      <button type="button" onClick={(event) => (event.currentTarget.focus(), void session.open())}>
        Extract pages
      </button>
      <button type="button" onClick={() => void session.open('2-3')}>
        Extract selection
      </button>
      <ExtractControls session={session} pageCount={pages} />
    </>
  );
}

function SplitHarness({ engine, delivery, pages = 5 }: { engine: Engine; delivery: Delivery; pages?: number }) {
  const session = useSplit(engine, openDocument(pages), delivery);
  return (
    <>
      <button type="button" onClick={() => void session.open()}>
        Split
      </button>
      <SplitControls session={session} pageCount={pages} />
    </>
  );
}

const extractDialog = () => screen.findByRole('dialog', { name: 'Extract pages' });
const splitDialog = () => screen.findByRole('dialog', { name: 'Split PDF' });
const field = (shown: HTMLElement, label: string) => within(shown).getByLabelText(label) as HTMLInputElement;

describe('ExtractControls', () => {
  it('asks the gates, then downloads the typed pages as -extract.pdf after the queued edits', async () => {
    const { engine, asked, extracted } = extractEngine();
    const { delivery, delivered } = recording();
    let settled = 0;
    render(<ExtractHarness engine={engine} delivery={delivery} settle={async () => void settled++} />);
    fireEvent.click(screen.getByRole('button', { name: 'Extract pages' }));

    const shown = await extractDialog();
    expect(asked).toEqual(['refusal:7', 'signed:7']);
    expect(within(shown).getByText('This document has 5 pages.')).toBeTruthy();
    const range = field(shown, 'Pages to extract');
    expect(range).toBe(window.document.activeElement);
    fireEvent.change(range, { target: { value: '2-3' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Extract' }));

    expect(await screen.findByText('Downloaded a-extract.pdf: 2 pages extracted to a new PDF.')).toBeTruthy();
    expect(settled).toBe(1);
    expect(extracted).toEqual([[1, 2]]);
    expect(delivered).toEqual([{ bytes: new Uint8Array([37, 1, 2]), name: 'a-extract.pdf', destination: { kind: 'newFile' } }]);
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.getByRole('button', { name: 'Extract pages' })).toBe(window.document.activeElement);
  });

  it('opens on a run it is given, as Organize gives its selection', async () => {
    const { engine } = extractEngine();
    render(<ExtractHarness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Extract selection' }));
    expect(field(await extractDialog(), 'Pages to extract').value).toBe('2-3');
  });

  it("keeps the dialog open with the core's sentence for a range it cannot read, and with a hint for none", async () => {
    const { engine, extracted } = extractEngine();
    const { delivery, delivered } = recording();
    render(<ExtractHarness engine={engine} delivery={delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Extract pages' }));
    const shown = await extractDialog();

    fireEvent.click(within(shown).getByRole('button', { name: 'Extract' }));
    expect(await within(shown).findByText('Type which pages to extract, for example 1-3,7.')).toBeTruthy();

    fireEvent.change(field(shown, 'Pages to extract'), { target: { value: '4-9' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Extract' }));
    expect(await within(shown).findByText('Page 9 does not exist.')).toBeTruthy();
    expect(field(shown, 'Pages to extract').value).toBe('4-9');
    expect(extracted).toEqual([]);
    expect(delivered).toEqual([]);
  });

  it('says why a document cannot have pages extracted and offers only Cancel', async () => {
    const { engine, asked } = extractEngine({ refusal: 'this document does not permit extracting its pages' });
    render(<ExtractHarness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Extract pages' }));
    const shown = await extractDialog();
    expect(within(shown).getByRole('alert').textContent).toBe('This document does not permit extracting its pages.');
    expect(asked).toEqual(['refusal:7']);
    expect(within(shown).getByRole('button', { name: 'Extract' })).toHaveProperty('disabled', true);
    expect(field(shown, 'Pages to extract').disabled).toBe(true);
    expect(within(shown).getByRole('button', { name: 'Cancel' })).toBe(window.document.activeElement);
    fireEvent.click(within(shown).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
  });

  it('adds the signature note for a signed source', async () => {
    const { engine } = extractEngine({ signed: true });
    render(<ExtractHarness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Extract pages' }));
    const shown = await extractDialog();
    fireEvent.change(field(shown, 'Pages to extract'), { target: { value: '1' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Extract' }));
    expect(await screen.findByText(/1 page extracted to a new PDF\. The original document is signed/)).toBeTruthy();
  });
});

describe('SplitControls', () => {
  it('downloads one PDF per part under the core names, in order', async () => {
    const { engine, extracted } = extractEngine();
    const { delivery, delivered } = recording();
    render(<SplitHarness engine={engine} delivery={delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    expect(within(shown).getByText('This document has 5 pages. Each cut starts a new file.')).toBeTruthy();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '2,4' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));

    expect(await screen.findByText('Split into 3 PDFs.')).toBeTruthy();
    expect(extracted).toEqual([[0, 1], [2, 3], [4]]);
    expect(delivered.map(({ name, destination }) => [name, destination.kind])).toEqual([
      ['a-part1.pdf', 'newFile'],
      ['a-part2.pdf', 'newFile'],
      ['a-part3.pdf', 'newFile'],
    ]);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('plans against the page count once the queued edits have landed, not the one of the click', async () => {
    const { engine, extracted } = extractEngine();
    let pages = 5;
    function Late() {
      // An Organize insert lands while the split waits: the render still says 5 pages.
      const session = useSplit(engine, openDocument(5), recording().delivery, async () => void (pages = 6), () => pages);
      return (
        <>
          <button type="button" onClick={() => void session.open()}>
            Split
          </button>
          <SplitControls session={session} pageCount={5} />
        </>
      );
    }
    render(<Late />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '5' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));
    expect(await screen.findByText('Split into 2 PDFs.')).toBeTruthy();
    // The new sixth page is the second part.
    expect(extracted).toEqual([[0, 1, 2, 3, 4], [5]]);
  });

  it("keeps the dialog open with the core's sentence for a cut it refuses", async () => {
    const { engine, extracted } = extractEngine();
    render(<SplitHarness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '5' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));
    expect(await within(shown).findByText('There is nothing after page 5, so the document cannot be split there.')).toBeTruthy();
    expect(extracted).toEqual([]);
  });

  it('refuses a one-page document up front, in the core words', async () => {
    const { engine } = extractEngine();
    render(<SplitHarness engine={engine} delivery={recording().delivery} pages={1} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    expect(within(shown).getByRole('alert').textContent).toBe('A document of one page cannot be split.');
    expect(within(shown).getByRole('button', { name: 'Split' })).toHaveProperty('disabled', true);
  });

  it('stops at the first part that fails and counts what was written', async () => {
    const { engine } = extractEngine({ failAt: 2 });
    const { delivery, delivered } = recording();
    render(<SplitHarness engine={engine} delivery={delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '2,4' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));
    expect(await screen.findByText('Split stopped: a-part2.pdf could not be made: The writer broke. 1 written.')).toBeTruthy();
    expect(delivered.map(({ name }) => name)).toEqual(['a-part1.pdf']);
  });

  it('stops after the part in hand when cancelled', async () => {
    const { engine } = extractEngine();
    const delivered: string[] = [];
    let release: () => void = () => {};
    const delivery: Delivery = {
      prepare: async () => null,
      deliver: async ({ name }) => {
        delivered.push(name);
        // The first part waits, so Cancel split can be pressed while it is in hand.
        if (delivered.length === 1) await new Promise<void>((resolve) => (release = resolve));
        return { how: 'downloaded', name, fellBack: null };
      },
    };
    render(<SplitHarness engine={engine} delivery={delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '2,4' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));
    expect(await screen.findByText('Writing part 1 of 3…')).toBeTruthy();
    fireEvent.click(screen.getByRole('button', { name: 'Cancel split' }));
    release();
    expect(await screen.findByText('Split cancelled. 1 written.')).toBeTruthy();
    expect(delivered).toEqual(['a-part1.pdf']);
  });

  it('asks the engine for at most one part ahead, so a cancel spares the parts after it', async () => {
    const { engine, extracted } = extractEngine();
    const delivered: string[] = [];
    let release: () => void = () => {};
    const delivery: Delivery = {
      prepare: async () => null,
      deliver: async ({ name }) => {
        delivered.push(name);
        if (delivered.length === 1) await new Promise<void>((resolve) => (release = resolve));
        return { how: 'downloaded', name, fellBack: null };
      },
    };
    render(<SplitHarness engine={engine} delivery={delivery} />);
    fireEvent.click(screen.getByRole('button', { name: 'Split' }));
    const shown = await splitDialog();
    fireEvent.change(field(shown, 'Split after page'), { target: { value: '2,4' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Split' }));
    await waitFor(() => expect(delivered).toEqual(['a-part1.pdf']));
    // Part 1 is being written: part 2 may be on its way, part 3 is not asked for.
    expect(extracted).toEqual([
      [0, 1],
      [2, 3],
    ]);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel split' }));
    release();
    expect(await screen.findByText('Split cancelled. 1 written.')).toBeTruthy();
    expect(extracted).toEqual([
      [0, 1],
      [2, 3],
    ]);
  });
});
