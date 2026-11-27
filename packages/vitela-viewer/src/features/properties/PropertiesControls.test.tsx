import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError, type DocumentProperties, type Edit, type EditedDocument, type Engine } from '@vitela/core';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { useRef } from 'react';
import { PropertiesControls } from './PropertiesControls';
import { propertiesBusy, useDocumentProperties, type PropertiesHost } from './useDocumentProperties';

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

const FILE: DocumentProperties = {
  title: 'Quarterly report',
  author: 'Ana',
  subject: null,
  keywords: null,
  creator: 'Writer',
  producer: null,
};

const edited: EditedDocument = { info: document.info, canUndo: true, canRedo: false, annotations: [], addedAnnotationId: null, formFields: [] };

interface EngineOptions {
  readonly refusal?: string | null;
  /** What the core answers an apply with. */
  readonly answer?: () => EditedDocument;
}

function propertiesEngine({ refusal = null, answer = () => edited }: EngineOptions = {}) {
  const log: string[] = [];
  const applied: Edit[] = [];
  let current = FILE;
  const engine: Engine = fakeEngine({
    documentProperties: async (handle) => (log.push(`read:${handle}`), current),
    metadataRefusal: async (handle) => (log.push(`refusal:${handle}`), refusal),
    applyEdit: async (handle, edit) => {
      log.push(`apply:${handle}`);
      applied.push(edit);
      const answered = answer();
      if (edit.kind === 'setDocumentProperties') current = edit.properties;
      return answered;
    },
  });
  return { engine, log, applied, setCurrent: (next: DocumentProperties) => (current = next) };
}

function host(overrides: Partial<PropertiesHost> = {}) {
  const reported: EditedDocument[] = [];
  let dirty = 0;
  const waiting = new Set<() => Promise<unknown>>();
  const value: PropertiesHost = {
    settleEdits: async () => {},
    waitForEdits: (settled) => (waiting.add(settled), () => void waiting.delete(settled)),
    onEdited: (_handle, answer) => (reported.push(answer), true),
    markDirty: () => void dirty++,
    ...overrides,
  };
  return { value, reported, dirty: () => dirty, waiting };
}

/** The workspace's wiring: the dialog in the bar, opened from the Tools menu (a stand-in entry here). */
function Harness({ engine, props }: { engine: Engine; props: PropertiesHost }) {
  const session = useDocumentProperties(engine, document, props);
  const tools = useRef<HTMLButtonElement>(null);
  const busy = propertiesBusy(session.activity);
  return (
    <>
      <button ref={tools} type="button" aria-disabled={busy} onClick={busy ? undefined : () => void session.open()}>
        Document properties
      </button>
      <PropertiesControls session={session} trigger={tools} />
    </>
  );
}

const trigger = () => screen.getByRole('button', { name: 'Document properties' });
const dialog = () => screen.findByRole('dialog', { name: 'Document properties' });
const box = (shown: HTMLElement, label: string) => within(shown).getByLabelText(label) as HTMLInputElement;

describe('PropertiesControls', () => {
  it('shows the properties the next save would write, and records the edited ones as one edit', async () => {
    const { engine, log, applied } = propertiesEngine();
    const { value, reported, dirty } = host();
    render(<Harness engine={engine} props={value} />);
    const button = trigger();
    button.focus();
    fireEvent.click(button);
    const shown = await dialog();
    expect(box(shown, 'Title').value).toBe('Quarterly report');
    expect(box(shown, 'Subject').value).toBe('');
    expect(box(shown, 'Title')).toBe(window.document.activeElement);

    fireEvent.change(box(shown, 'Title'), { target: { value: 'Annual report' } });
    fireEvent.change(box(shown, 'Author'), { target: { value: 'Luis' } });
    fireEvent.change(box(shown, 'Creator'), { target: { value: '' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Apply' }));

    expect(await screen.findByText('Document properties updated. Changes are pending save.')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(applied).toEqual([
      {
        kind: 'setDocumentProperties',
        properties: { title: 'Annual report', author: 'Luis', subject: null, keywords: null, creator: null, producer: null },
      },
    ]);
    // Read at open, and again right before the change: an undo may have landed.
    expect(log).toEqual(['read:7', 'refusal:7', 'read:7', 'apply:7']);
    expect(reported).toEqual([edited]);
    expect(dirty()).toBe(1);
    await waitFor(() => expect(button).toBe(window.document.activeElement));
  });

  it('records nothing when the boxes say what the document has', async () => {
    const { engine, applied } = propertiesEngine();
    const { value, dirty } = host();
    render(<Harness engine={engine} props={value} />);
    fireEvent.click(trigger());
    const shown = await dialog();
    fireEvent.click(within(shown).getByRole('button', { name: 'Apply' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(applied).toEqual([]);
    expect(dirty()).toBe(0);
    expect(screen.queryByText(/updated/)).toBeNull();
    // Properties opens again after an Apply that changed nothing.
    fireEvent.click(trigger());
    expect(await dialog()).toBeTruthy();
  });

  it('waits for the edits asked for before Apply, and compares with the properties as they are then', async () => {
    const { engine, applied, setCurrent } = propertiesEngine();
    let land: () => void = () => {};
    const queued = new Promise<void>((resolve) => (land = resolve));
    const { value } = host({ settleEdits: () => queued });
    render(<Harness engine={engine} props={value} />);
    fireEvent.click(trigger());
    const shown = await dialog();
    fireEvent.change(box(shown, 'Title'), { target: { value: 'Annual report' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Apply' }));
    expect(await within(shown).findByRole('button', { name: 'Applying…' })).toBeTruthy();
    expect(applied).toEqual([]);
    // A queued undo takes the author away before the change goes in.
    setCurrent({ ...FILE, author: null });
    land();
    await waitFor(() => expect(applied).toHaveLength(1));
    expect(applied[0]).toMatchObject({ properties: { title: 'Annual report', author: 'Ana' } });
  });

  it('hands its apply to the save, so a save waits for it', async () => {
    const { engine } = propertiesEngine();
    const { value, waiting } = host();
    render(<Harness engine={engine} props={value} />);
    await waitFor(() => expect(waiting.size).toBe(1));
    cleanup();
    expect(waiting.size).toBe(0);
  });

  it('keeps the dialog and what was typed when the core refuses the change', async () => {
    const { engine } = propertiesEngine({
      answer: () => {
        throw new EngineError('EditNotPermitted', 'this document does not permit metadata changes');
      },
    });
    const { value, dirty } = host();
    render(<Harness engine={engine} props={value} />);
    fireEvent.click(trigger());
    const shown = await dialog();
    fireEvent.change(box(shown, 'Title'), { target: { value: 'Mine' } });
    fireEvent.click(within(shown).getByRole('button', { name: 'Apply' }));
    expect((await within(shown).findByRole('alert')).textContent).toBe('This document does not permit metadata changes.');
    expect(box(shown, 'Title').value).toBe('Mine');
    expect(dirty()).toBe(0);
  });

  it('is read-only, with only Close, when the document withholds modification', async () => {
    const { engine, applied } = propertiesEngine({ refusal: 'this document does not permit metadata changes' });
    const { value } = host();
    render(<Harness engine={engine} props={value} />);
    fireEvent.click(trigger());
    const shown = await dialog();
    expect(within(shown).getByRole('alert').textContent).toBe('This document does not permit metadata changes.');
    expect(box(shown, 'Title').value).toBe('Quarterly report');
    expect(box(shown, 'Title').readOnly).toBe(true);
    expect(within(shown).queryByRole('button', { name: 'Apply' })).toBeNull();
    fireEvent.click(within(shown).getByRole('button', { name: 'Close' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(applied).toEqual([]);
  });

  it('gives the focus back to what opened it (the Tools menu button)', async () => {
    const { engine } = propertiesEngine();
    render(<Harness engine={engine} props={host().value} />);
    fireEvent.click(trigger());
    const shown = await dialog();
    fireEvent.click(within(shown).getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(trigger()).toBe(window.document.activeElement));
  });

  it('says so in the bar when the properties cannot be read', async () => {
    const engine = fakeEngine({
      documentProperties: async () => {
        throw new EngineError('DocumentNotFound', 'document 7 is not open');
      },
      metadataRefusal: async () => null,
    });
    render(<Harness engine={engine} props={host().value} />);
    fireEvent.click(trigger());
    expect(await screen.findByText('Could not read the document properties: document 7 is not open')).toBeTruthy();
    expect(screen.queryByRole('dialog')).toBeNull();
  });
});
