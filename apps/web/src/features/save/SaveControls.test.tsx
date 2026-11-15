import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError } from '../../core/errors';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import { SaveControls } from './SaveControls';
import type { SaveOutcome } from './saveBytes';

const document: OpenDocument = {
  name: 'a.pdf',
  handle: 7,
  generation: 1,
  info: { pageCount: 1, pages: [{ widthPt: 612, heightPt: 792, rotationDegrees: 0 }], textExtractionAllowed: true },
};

const SIGNATURE_TITLE = "Saving will break this document's signature";

// jsdom has no modal <dialog>: model just enough of it for the component.
beforeAll(() => {
  HTMLDialogElement.prototype.showModal ??= function (this: HTMLDialogElement) {
    this.setAttribute('open', '');
  };
  HTMLDialogElement.prototype.close ??= function (this: HTMLDialogElement) {
    this.removeAttribute('open');
  };
});
afterEach(cleanup);

const bytes = new Uint8Array([37]);
const deliver = (outcome: SaveOutcome) => async () => outcome;
const downloaded: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: false };

describe('SaveControls', () => {
  it('saves on click and says the file was downloaded', async () => {
    const engine = fakeEngine({ saveToBytes: async () => bytes });
    render(<SaveControls engine={engine} document={document} deliver={deliver(downloaded)} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Downloaded a.pdf.')).toBeTruthy();
  });

  it('says when it wrote over the opened file', async () => {
    const engine = fakeEngine({ saveToBytes: async () => bytes });
    render(<SaveControls engine={engine} document={document} deliver={deliver({ how: 'wrote', name: 'a.pdf' })} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Saved over a.pdf.')).toBeTruthy();
  });

  it('says plainly that it downloaded because the opened file could not be written', async () => {
    const engine = fakeEngine({ saveToBytes: async () => bytes });
    const outcome: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: true };
    render(<SaveControls engine={engine} document={document} deliver={deliver(outcome)} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(await screen.findByText('Could not write over the opened file, so a.pdf was downloaded instead.')).toBeTruthy();
  });

  it('shows an engine failure as an alert', async () => {
    const engine = fakeEngine({
      saveToBytes: async () => {
        throw new EngineError('SaveFailed', 'the document could not be written');
      },
    });
    render(<SaveControls engine={engine} document={document} deliver={deliver(downloaded)} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect((await screen.findByRole('alert')).textContent).toBe('Could not save a.pdf: the document could not be written');
  });

  it('asks before breaking a signature; Save anyway then saves with the acknowledgement', async () => {
    const asked: boolean[] = [];
    const engine = fakeEngine({
      saveToBytes: async (_handle, invalidate) => {
        asked.push(invalidate);
        if (!invalidate) throw new EngineError('SignaturesWouldBeInvalidated', 'signed');
        return bytes;
      },
    });
    render(<SaveControls engine={engine} document={document} deliver={deliver(downloaded)} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));

    const dialog = await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
    expect(dialog.textContent).toContain('cancel and save to a different file');
    fireEvent.click(screen.getByRole('button', { name: 'Save anyway' }));
    await waitFor(() => expect(screen.getByText('Downloaded a.pdf.')).toBeTruthy());
    expect(asked).toEqual([false, true]);
  });

  it('Cancel on the signature question saves nothing', async () => {
    const asked: boolean[] = [];
    const engine = fakeEngine({
      saveToBytes: async (_handle, invalidate) => {
        asked.push(invalidate);
        throw new EngineError('SignaturesWouldBeInvalidated', 'signed');
      },
    });
    render(<SaveControls engine={engine} document={document} deliver={deliver(downloaded)} />);
    fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(asked).toEqual([false]);
  });
});
