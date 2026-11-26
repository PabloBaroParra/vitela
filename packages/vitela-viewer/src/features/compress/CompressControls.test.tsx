import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError, type CompressedDocument, type Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { DeliverRequest, Delivery } from '../save/delivery';
import type { SaveOutcome } from '../save/outcome';
import { CompressControls } from './CompressControls';
import { useCompress } from './useCompress';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: {
    pageCount: 1,
    pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }],
    pageIds: [0],
    textExtractionAllowed: true,
    documentAssemblyAllowed: true,
    blocks: [],
    importRefusal: null,
    annotationEditingAllowed: true,
    formFillAllowed: true,
  },
};

beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

const smaller: CompressedDocument = {
  bytes: new Uint8Array([37, 80]),
  beforeBytes: 2_400_000,
  afterBytes: 1_200_000,
  savedBytes: 1_200_000,
  reduced: true,
  refusals: [],
};
const same: CompressedDocument = { ...smaller, afterBytes: 2_400_000, savedBytes: 0, reduced: false };

/** A delivery that records what it was handed and answers `outcome`. */
function recording(outcome?: (request: DeliverRequest) => SaveOutcome | Promise<SaveOutcome>) {
  const delivered: DeliverRequest[] = [];
  const delivery: Delivery = {
    prepare: async () => {
      throw new Error('a compressed copy needs no destination prepared');
    },
    deliver: async (request) => {
      delivered.push(request);
      return outcome ? outcome(request) : { how: 'downloaded', name: request.name, fellBack: null };
    },
  };
  return { delivery, delivered };
}

interface EngineOptions {
  readonly refusal?: Awaited<ReturnType<Engine['compressionRefusal']>>;
  readonly signed?: boolean | Error;
  readonly result?: CompressedDocument | Error;
}

function compressEngine({ refusal = null, signed = false, result = smaller }: EngineOptions = {}) {
  const asked: string[] = [];
  const runs: [number, string, boolean][] = [];
  const engine = fakeEngine({
    compressPresets: async () => (asked.push('presets'), ['lossless', 'balanced', 'small']),
    compressionRefusal: async (handle) => (asked.push(`refusal:${handle}`), refusal),
    compressionInvalidatesSignatures: async (handle) => {
      asked.push(`signed:${handle}`);
      if (signed instanceof Error) throw signed;
      return signed;
    },
    saveCompressedToBytes: async (handle, preset, invalidate) => {
      runs.push([handle, preset, invalidate]);
      if (result instanceof Error) throw result;
      return result;
    },
  });
  return { engine, asked, runs };
}

function Harness({
  engine,
  delivery,
  settle,
  focusFallback,
}: {
  engine: Engine;
  delivery: Delivery;
  settle?: () => Promise<unknown>;
  focusFallback?: () => void;
}) {
  const session = useCompress(engine, document, delivery, settle);
  return <CompressControls session={session} documentName={document.name} focusFallback={focusFallback} />;
}

const compressButton = () => screen.getByRole('button', { name: 'Compress' });
const dialog = () => screen.findByRole('dialog', { name: 'Compress PDF' });

describe('CompressControls', () => {
  it('asks the gates, offers the core presets with Balanced chosen, and downloads a smaller copy', async () => {
    const { engine, asked, runs } = compressEngine();
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(compressButton());

    const shown = await dialog();
    expect(asked).toEqual(['presets', 'refusal:7', 'signed:7']);
    const radios = within(shown).getAllByRole('radio') as HTMLInputElement[];
    expect(radios.map((radio) => radio.value)).toEqual(['lossless', 'balanced', 'small']);
    expect(within(shown).getByRole('radio', { name: /Balanced/ })).toHaveProperty('checked', true);
    expect(within(shown).getByText('Writes a smaller copy. The open document stays as it is.')).toBeTruthy();

    fireEvent.click(within(shown).getByRole('button', { name: 'Compress' }));
    expect(
      await screen.findByText('2.4 MB uncompressed → 1.2 MB compressed (50% smaller). Downloaded a-compressed.pdf.'),
    ).toBeTruthy();
    expect(runs).toEqual([[7, 'balanced', false]]);
    expect(delivered).toEqual([{ bytes: smaller.bytes, name: 'a-compressed.pdf', destination: { kind: 'newFile' } }]);
    expect(screen.queryByRole('dialog')).toBeNull();
  });

  it('gives the focus to the fallback when its own button is not shown (a phone shows More tools instead)', async () => {
    const { engine } = compressEngine();
    const { delivery } = recording();
    let fallback = 0;
    render(<Harness engine={engine} delivery={delivery} focusFallback={() => void fallback++} />);
    Object.defineProperty(compressButton(), 'checkVisibility', { value: () => false });
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(fallback).toBe(1);
  });

  it('runs the preset the user picked', async () => {
    const { engine, runs } = compressEngine();
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    const shown = await dialog();
    fireEvent.click(within(shown).getByRole('radio', { name: /Small/ }));
    fireEvent.click(within(shown).getByRole('button', { name: 'Compress' }));
    await waitFor(() => expect(runs).toEqual([[7, 'small', false]]));
  });

  it('writes nothing when nothing smaller came out, and says so', async () => {
    const { engine } = compressEngine({ result: same });
    const { delivery, delivered } = recording();
    render(<Harness engine={engine} delivery={delivery} />);
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Compress' }));
    expect(
      await screen.findByText(
        'Nothing to gain: this document compresses to the same 2.4 MB an uncompressed save writes, so no file was written.',
      ),
    ).toBeTruthy();
    expect(delivered).toEqual([]);
  });

  it('refuses an encrypted document in the dialog, before anything runs', async () => {
    const message = "this document's password protection does not allow it to be rewritten, so it cannot be compressed";
    const { engine, asked, runs } = compressEngine({ refusal: { kind: 'encryptedDocumentNotRewritable', message } });
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe(
      "This document's password protection does not allow it to be rewritten, so it cannot be compressed.",
    );
    // The signature question is not asked of a document that cannot be compressed.
    expect(asked).toEqual(['presets', 'refusal:7']);
    const run = within(shown).getByRole('button', { name: 'Compress' });
    expect(run).toHaveProperty('disabled', true);
    for (const radio of within(shown).getAllByRole('radio')) expect(radio).toHaveProperty('disabled', true);
    fireEvent.click(run);
    expect(runs).toEqual([]);
  });

  it('warns about a signature in the dialog, and "Compress anyway" is the consent', async () => {
    const { engine, runs } = compressEngine({ signed: true });
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    const shown = await dialog();
    expect(within(shown).getByText(/This document is signed\./)).toBeTruthy();
    expect(within(shown).queryByRole('button', { name: 'Compress' })).toBeNull();
    fireEvent.click(within(shown).getByRole('button', { name: 'Compress anyway' }));
    await waitFor(() => expect(runs).toEqual([[7, 'balanced', true]]));
  });

  it('refuses rather than guessing "unsigned" when the signature question fails', async () => {
    const { engine, runs } = compressEngine({ signed: new EngineError('SaveFailed', 'could not read the signatures') });
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe('Could not read the signatures.');
    expect(within(shown).getByRole('button', { name: 'Compress' })).toHaveProperty('disabled', true);
    expect(runs).toEqual([]);
  });

  it('Cancel closes the dialog and runs nothing', async () => {
    const { engine, runs } = compressEngine();
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(runs).toEqual([]);
    expect(screen.queryByRole('status')).toBeNull();
  });

  it('waits for the edits already asked for before it compresses', async () => {
    const { engine, runs } = compressEngine();
    let release!: () => void;
    const settled = new Promise<void>((resolve) => (release = resolve));
    render(<Harness engine={engine} delivery={recording().delivery} settle={() => settled} />);
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Compress' }));
    expect(await screen.findByText('Compressing…')).toBeTruthy();
    expect(runs).toEqual([]);
    release();
    await waitFor(() => expect(runs).toEqual([[7, 'balanced', false]]));
  });

  it('shows a failure of the engine and of the delivery', async () => {
    const failing = compressEngine({ result: new EngineError('SaveFailed', 'out of memory') });
    render(<Harness engine={failing.engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Compress' }));
    expect((await screen.findByRole('alert')).textContent).toBe('Could not compress a.pdf: out of memory');
    cleanup();

    const rejecting = recording(() => {
      throw new Error('the host refused it');
    });
    render(<Harness engine={compressEngine().engine} delivery={rejecting.delivery} />);
    fireEvent.click(compressButton());
    fireEvent.click(within(await dialog()).getByRole('button', { name: 'Compress' }));
    expect((await screen.findByRole('alert')).textContent).toBe('Could not compress a.pdf: the host refused it');
  });

  it('keeps the button from starting a second run while one is under way', async () => {
    const { engine, asked } = compressEngine();
    render(<Harness engine={engine} delivery={recording().delivery} />);
    fireEvent.click(compressButton());
    fireEvent.click(compressButton());
    const shown = await dialog();
    expect(asked.filter((question) => question === 'presets')).toHaveLength(1);
    const toolbar = screen.getAllByRole('button', { name: 'Compress' }).find((button) => !shown.contains(button))!;
    expect(toolbar.getAttribute('aria-disabled')).toBe('true');
  });
});
