import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { useRef } from 'react';
import { EngineError, type Edit, type EditedDocument, type Engine, type WatermarkDefaults } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import { MAX_STAMP_BYTES } from '../annotations/stamps';
import type { OpenDocument } from '../document/useDocument';
import { WatermarkControls } from './WatermarkControls';
import { useWatermark, watermarkBusy, type WatermarkHost } from './useWatermark';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: {
    pageCount: 3,
    pages: [
      { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
      { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
      { widthPt: 612, heightPt: 792, rotationDegrees: 0 },
    ],
    pageIds: [0, 1, 2],
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

const LOOK: WatermarkDefaults = {
  fontSize: 60,
  textOpacity: 0.3,
  textRotationDegrees: 45,
  textBehind: false,
  pictureScale: 0.5,
  pictureOpacity: 0.3,
  pictureRotationDegrees: 0,
  pictureBehind: true,
};

const edited: EditedDocument = { info: document.info, canUndo: true, canRedo: false, annotations: [], addedAnnotationId: null, formFields: [] };

interface EngineOptions {
  readonly refusal?: string | null;
  /** What the core answers an Add with. */
  readonly answer?: (edit: Edit) => EditedDocument;
}

function watermarkEngine({ refusal = null, answer = () => edited }: EngineOptions = {}) {
  const log: string[] = [];
  const applied: Edit[] = [];
  const engine: Engine = fakeEngine({
    watermarkDefaults: async () => (log.push('defaults'), LOOK),
    watermarkRefusal: async (handle) => (log.push(`refusal:${handle}`), refusal),
    parsePageSelection: async (input, total) => {
      log.push(`parse:${input}:${total}`);
      if (input === 'nine') throw new EngineError('InvalidPageSelection', 'page selection is not valid');
      return [0, total - 1];
    },
    applyEdit: async (handle, edit) => {
      log.push(`apply:${handle}`);
      applied.push(edit);
      return answer(edit);
    },
  });
  return { engine, log, applied };
}

function host(overrides: Partial<WatermarkHost> = {}) {
  const reported: EditedDocument[] = [];
  let dirty = 0;
  const waiting = new Set<() => Promise<unknown>>();
  const value: WatermarkHost = {
    settleEdits: async () => {},
    waitForEdits: (settled) => (waiting.add(settled), () => void waiting.delete(settled)),
    onEdited: (_handle, answer) => (reported.push(answer), true),
    markDirty: () => void dirty++,
    currentPage: () => 1,
    pageCountNow: () => 3,
    ...overrides,
  };
  return { value, reported, dirty: () => dirty, waiting };
}

/** The workspace's wiring: the dialog in the bar, opened from the Tools menu (a stand-in entry here). */
function Harness({ engine, props }: { engine: Engine; props: WatermarkHost }) {
  const session = useWatermark(engine, document, props);
  const tools = useRef<HTMLButtonElement>(null);
  const busy = watermarkBusy(session.activity);
  return (
    <>
      <button ref={tools} type="button" aria-disabled={busy} onClick={busy ? undefined : () => void session.open()}>
        Watermark
      </button>
      <WatermarkControls session={session} trigger={tools} />
    </>
  );
}

const trigger = () => screen.getByRole('button', { name: 'Watermark' });
const dialog = () => screen.findByRole('dialog', { name: 'Add watermark' });
const add = (shown: HTMLElement) => fireEvent.click(within(shown).getByRole('button', { name: 'Add' }));

async function opened(engine: Engine, props: WatermarkHost) {
  render(<Harness engine={engine} props={props} />);
  fireEvent.click(trigger());
  return dialog();
}

describe('WatermarkControls', () => {
  it("starts from the core's look and stamps the text on every page as one edit", async () => {
    const { engine, log, applied } = watermarkEngine();
    const { value, reported, dirty } = host();
    render(<Harness engine={engine} props={value} />);
    const button = trigger();
    button.focus();
    fireEvent.click(button);
    const shown = await dialog();
    expect((within(shown).getByLabelText('Watermark text') as HTMLInputElement).value).toBe('DRAFT');
    expect(within(shown).getByText('Size: 60 pt')).toBeTruthy();
    expect(within(shown).getByText('Opacity: 30%')).toBeTruthy();
    expect(within(shown).getByText('Angle: 45°')).toBeTruthy();
    expect(within(shown).getByRole('radio', { name: 'Over the page content' })).toHaveProperty('checked', true);
    expect(within(shown).getByRole('radio', { name: 'All pages' })).toHaveProperty('checked', true);

    fireEvent.change(within(shown).getByLabelText('Watermark text'), { target: { value: 'CONFIDENTIAL' } });
    fireEvent.change(within(shown).getByRole('slider', { name: 'Opacity' }), { target: { value: '50' } });
    fireEvent.click(within(shown).getByRole('radio', { name: 'Behind the page content' }));
    add(shown);

    expect(await screen.findByText('Watermark added to 3 pages.')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(applied).toEqual([
      {
        kind: 'addWatermark',
        pages: [0, 1, 2],
        mark: { kind: 'text', text: 'CONFIDENTIAL', fontSize: 60 },
        opacity: 0.5,
        rotationDegrees: 45,
        behind: true,
      },
    ]);
    expect(log).toEqual(['defaults', 'refusal:7', 'apply:7']);
    expect(reported).toEqual([edited]);
    expect(dirty()).toBe(1);
    await waitFor(() => expect(button).toBe(window.document.activeElement));
  });

  it('stamps the page the viewer last showed, kept inside the page count the last edit left', async () => {
    const { engine, applied } = watermarkEngine();
    const shown = await opened(engine, host({ currentPage: () => 5, pageCountNow: () => 2 }).value);
    fireEvent.click(within(shown).getByRole('radio', { name: 'Current page' }));
    add(shown);
    expect(await screen.findByText('Watermark added to 1 page.')).toBeTruthy();
    expect(applied[0]).toMatchObject({ pages: [1] });
  });

  it("reads a typed range with the core's grammar, after the edits asked for before Add", async () => {
    const { engine, log, applied } = watermarkEngine();
    let land: () => void = () => {};
    const queued = new Promise<void>((resolve) => (land = resolve));
    let count = 3;
    const shown = await opened(engine, host({ settleEdits: () => queued, pageCountNow: () => count }).value);
    fireEvent.change(within(shown).getByLabelText('Page range'), { target: { value: '1-2' } });
    expect(within(shown).getByRole('radio', { name: 'Custom range' })).toHaveProperty('checked', true);
    add(shown);
    expect(await within(shown).findByRole('button', { name: 'Adding…' })).toBeTruthy();
    expect(applied).toEqual([]);
    // A queued deletion lands first: the range is read against what it left.
    count = 2;
    land();
    await waitFor(() => expect(applied).toHaveLength(1));
    expect(log).toContain('parse:1-2:2');
    expect(applied[0]).toMatchObject({ pages: [0, 1] });
  });

  it('refuses blank text, an empty range or a bad one in the dialog, keeping the draft', async () => {
    const { engine, applied } = watermarkEngine();
    const { value, dirty } = host();
    const shown = await opened(engine, value);
    fireEvent.change(within(shown).getByLabelText('Watermark text'), { target: { value: '   ' } });
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe('Type the text to stamp on the pages.');

    fireEvent.change(within(shown).getByLabelText('Watermark text'), { target: { value: 'COPY' } });
    fireEvent.click(within(shown).getByRole('radio', { name: 'Custom range' }));
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe('Type which pages to stamp, for example 1-3,7.');

    fireEvent.change(within(shown).getByLabelText('Page range'), { target: { value: 'nine' } });
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe('Page selection is not valid.');
    expect((within(shown).getByLabelText('Watermark text') as HTMLInputElement).value).toBe('COPY');
    expect(applied).toEqual([]);
    expect(dirty()).toBe(0);
  });

  it("says which character the core's font cannot show, and keeps the dialog open", async () => {
    const { engine } = watermarkEngine({
      answer: () => {
        throw new EngineError('EncodingGap', "font Helvetica cannot represent '日'");
      },
    });
    const { value, dirty } = host();
    const shown = await opened(engine, value);
    fireEvent.change(within(shown).getByLabelText('Watermark text'), { target: { value: '日本' } });
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe(
      "Font Helvetica cannot represent '日'. Use Latin letters, digits and punctuation.",
    );
    expect((within(shown).getByLabelText('Watermark text') as HTMLInputElement).value).toBe('日本');
    expect(dirty()).toBe(0);
  });

  it("stamps a picture: the switch takes the picture's angle and placement, the bytes go as picked", async () => {
    const { engine, applied } = watermarkEngine({
      answer: (edit) => {
        if (edit.kind === 'addWatermark' && edit.mark.kind === 'picture' && edit.mark.image.length === 1) {
          throw new EngineError('InvalidImage', 'invalid image bytes: unknown format');
        }
        return edited;
      },
    });
    const shown = await opened(engine, host().value);
    fireEvent.change(within(shown).getByRole('slider', { name: 'Opacity' }), { target: { value: '60' } });
    fireEvent.click(within(shown).getByRole('radio', { name: 'Picture' }));
    expect(within(shown).getByText('Angle: 0°')).toBeTruthy();
    expect(within(shown).getByText('Opacity: 60%')).toBeTruthy();
    expect(within(shown).getByText('Size: 50% of the page')).toBeTruthy();
    expect(within(shown).getByRole('radio', { name: 'Behind the page content' })).toHaveProperty('checked', true);
    expect(within(shown).getByText('No picture chosen. PNG or JPEG.')).toBeTruthy();
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe('Choose a PNG or JPEG picture to stamp on the pages.');

    const picker = within(shown).getByLabelText('Choose picture…');
    fireEvent.change(picker, { target: { files: [new File([new Uint8Array([1])], 'bad.png', { type: 'image/png' })] } });
    expect(await within(shown).findByText('bad.png (1 KB).')).toBeTruthy();
    add(shown);
    expect((await within(shown).findByRole('alert')).textContent).toBe(
      'That file could not be read as a PNG or JPEG picture. Choose another one.',
    );

    const logo = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    fireEvent.change(within(shown).getByLabelText('Choose another picture…'), {
      target: { files: [new File([logo], 'logo.png', { type: 'image/png' })] },
    });
    expect(await within(shown).findByText('logo.png (1 KB).')).toBeTruthy();
    add(shown);
    expect(await screen.findByText('Watermark added to 3 pages.')).toBeTruthy();
    expect(applied.at(-1)).toEqual({
      kind: 'addWatermark',
      pages: [0, 1, 2],
      mark: { kind: 'picture', image: logo, scale: 0.5 },
      opacity: 0.6,
      rotationDegrees: 0,
      behind: true,
    });
  });

  it('refuses a picture over the image-stamp cap without reading it, keeping the earlier one', async () => {
    const { engine, applied } = watermarkEngine();
    const shown = await opened(engine, host().value);
    fireEvent.click(within(shown).getByRole('radio', { name: 'Picture' }));
    const logo = new Uint8Array([0x89, 0x50, 0x4e, 0x47]);
    fireEvent.change(within(shown).getByLabelText('Choose picture…'), {
      target: { files: [new File([logo], 'logo.png', { type: 'image/png' })] },
    });
    expect(await within(shown).findByText('logo.png (1 KB).')).toBeTruthy();

    const huge = new File([new Uint8Array([1])], 'huge.png', { type: 'image/png' });
    Object.defineProperty(huge, 'size', { value: MAX_STAMP_BYTES + 1 });
    let read = false;
    huge.arrayBuffer = async () => ((read = true), new ArrayBuffer(1));
    fireEvent.change(within(shown).getByLabelText('Choose another picture…'), { target: { files: [huge] } });
    expect((await within(shown).findByRole('alert')).textContent).toBe(
      'Pictures larger than 20 MB cannot be stamped on the pages. Choose a smaller one.',
    );
    expect(read).toBe(false);
    expect(within(shown).getByText('logo.png (1 KB).')).toBeTruthy();
    add(shown);
    expect(await screen.findByText('Watermark added to 3 pages.')).toBeTruthy();
    expect(applied.at(-1)).toMatchObject({ mark: { kind: 'picture', image: logo } });
  });

  it('offers only Close, with the core sentence, when the document cannot take a stamp', async () => {
    const { engine, applied } = watermarkEngine({ refusal: 'content editing is not permitted' });
    const shown = await opened(engine, host().value);
    expect(within(shown).getByRole('alert').textContent).toBe('Content editing is not permitted.');
    expect(within(shown).queryByRole('button', { name: 'Add' })).toBeNull();
    expect((within(shown).getByLabelText('Watermark text') as HTMLInputElement).disabled).toBe(true);
    fireEvent.click(within(shown).getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(applied).toEqual([]);
    await waitFor(() => expect(trigger()).toBe(window.document.activeElement));
  });

  it('hands its Add to the save, so a save waits for it', async () => {
    const { engine } = watermarkEngine();
    const { value, waiting } = host();
    render(<Harness engine={engine} props={value} />);
    await waitFor(() => expect(waiting.size).toBe(1));
    cleanup();
    expect(waiting.size).toBe(0);
  });

  it('says so in the bar when the dialog cannot be opened', async () => {
    const engine = fakeEngine({
      watermarkDefaults: async () => LOOK,
      watermarkRefusal: async () => {
        throw new EngineError('DocumentNotFound', 'document 7 is not open');
      },
    });
    render(<Harness engine={engine} props={host().value} />);
    fireEvent.click(trigger());
    expect(await screen.findByText('Could not open Add watermark: document 7 is not open')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
