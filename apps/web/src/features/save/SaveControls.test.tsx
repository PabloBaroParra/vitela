import { act, cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeAll, describe, expect, it } from 'vitest';
import { EngineError } from '../../core/errors';
import type { Engine } from '../../core/types';
import { fakeEngine } from '../../test/fakeEngine';
import type { OpenDocument } from '../document/useDocument';
import type { Delivery, Destination, SaveIntent } from './delivery';
import type { SaveOutcome } from './outcome';
import { SaveControls } from './SaveControls';
import { useSave, type SaveSession } from './useSave';

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
const downloaded: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: null };

/** A delivery that reports `outcome` (or cancels when `null`), noting the intents asked for. */
function delivery(outcome: SaveOutcome | null, intents: SaveIntent[] = []): Delivery {
  const destination: Destination = { kind: 'overwrite', handle: undefined, permission: 'granted' };
  return {
    prepare: async (intent) => (intents.push(intent), outcome ? destination : null),
    deliver: async () => outcome ?? downloaded,
  };
}

/** The workspace's role: owns the save state and lends it to the controls. */
function Harness({ engine, deliveryOf, onSession }: { engine: Engine; deliveryOf: Delivery; onSession?: (s: SaveSession) => void }) {
  const session = useSave(engine, document, deliveryOf);
  onSession?.(session);
  return <SaveControls session={session} documentName={document.name} />;
}

const renderControls = (engine: Engine, deliveryOf: Delivery, onSession?: (s: SaveSession) => void) =>
  render(<Harness engine={engine} deliveryOf={deliveryOf} onSession={onSession} />);

const signedEngine = (asked: boolean[] = []) =>
  fakeEngine({
    saveToBytes: async (_handle, invalidate) => {
      asked.push(invalidate);
      if (!invalidate) throw new EngineError('SignaturesWouldBeInvalidated', 'signed');
      return bytes;
    },
  });

const saveButton = () => screen.getByRole('button', { name: 'Save' });

describe('SaveControls', () => {
  it('saves on click and says the file was downloaded', async () => {
    renderControls(fakeEngine({ saveToBytes: async () => bytes }), delivery(downloaded));
    fireEvent.click(saveButton());
    expect(await screen.findByText('Downloaded a.pdf.')).toBeTruthy();
  });

  it('says when it wrote over the opened file', async () => {
    renderControls(fakeEngine({ saveToBytes: async () => bytes }), delivery({ how: 'wrote', name: 'a.pdf' }));
    fireEvent.click(saveButton());
    expect(await screen.findByText('Saved over a.pdf.')).toBeTruthy();
  });

  it('says plainly that it downloaded because the opened file could not be written', async () => {
    const outcome: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: 'write' };
    renderControls(fakeEngine({ saveToBytes: async () => bytes }), delivery(outcome));
    fireEvent.click(saveButton());
    expect(await screen.findByText('Could not write over the opened file, so a.pdf was downloaded instead.')).toBeTruthy();
  });

  it('says plainly that it downloaded because the write permission was refused', async () => {
    const outcome: SaveOutcome = { how: 'downloaded', name: 'a.pdf', fellBack: 'permission' };
    renderControls(fakeEngine({ saveToBytes: async () => bytes }), delivery(outcome));
    fireEvent.click(saveButton());
    expect(
      await screen.findByText('Permission to write over the opened file was refused, so a.pdf was downloaded instead.'),
    ).toBeTruthy();
  });

  it('shows an engine failure as an alert', async () => {
    const engine = fakeEngine({
      saveToBytes: async () => {
        throw new EngineError('SaveFailed', 'the document could not be written');
      },
    });
    renderControls(engine, delivery(downloaded));
    fireEvent.click(saveButton());
    expect((await screen.findByRole('alert')).textContent).toBe('Could not save a.pdf: the document could not be written');
  });

  it('asks before breaking a signature; Save anyway then saves with the acknowledgement', async () => {
    const asked: boolean[] = [];
    const intents: SaveIntent[] = [];
    renderControls(signedEngine(asked), delivery(downloaded, intents));
    fireEvent.click(saveButton());

    const dialog = await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
    expect(dialog.textContent).toContain('Save a copy to leave the original file');
    fireEvent.click(screen.getByRole('button', { name: 'Save anyway' }));
    await waitFor(() => expect(screen.getByText('Downloaded a.pdf.')).toBeTruthy());
    expect(asked).toEqual([false, true]);
    expect(intents).toEqual(['overwrite', 'overwrite']);
  });

  it('Save a copy saves with the acknowledgement as a COPY, and says the opened file was not changed', async () => {
    const asked: boolean[] = [];
    const intents: SaveIntent[] = [];
    renderControls(signedEngine(asked), delivery({ how: 'copied', name: 'a (copy).pdf' }, intents));
    fireEvent.click(saveButton());
    await screen.findByRole('dialog', { name: SIGNATURE_TITLE });

    fireEvent.click(screen.getByRole('button', { name: 'Save a copy' }));
    expect(await screen.findByText('Downloaded a copy as a (copy).pdf. The opened file was not changed.')).toBeTruthy();
    expect(asked).toEqual([false, true]);
    expect(intents).toEqual(['overwrite', 'copy']);
  });

  it('dismissing the save dialog of a copy cancels the save with a neutral status', async () => {
    const asked: boolean[] = [];
    // The first prepare (plain Save) goes through; the copy's is dismissed.
    let prepares = 0;
    const dismissing: Delivery = {
      prepare: async () => (++prepares === 1 ? { kind: 'overwrite', handle: undefined, permission: 'granted' } : null),
      deliver: async () => downloaded,
    };
    renderControls(signedEngine(asked), dismissing);
    fireEvent.click(saveButton());
    await screen.findByRole('dialog', { name: SIGNATURE_TITLE });

    fireEvent.click(screen.getByRole('button', { name: 'Save a copy' }));
    expect(await screen.findByText('Save cancelled.')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
    expect(asked).toEqual([false]);
  });

  it('Cancel on the signature question saves nothing', async () => {
    const asked: boolean[] = [];
    renderControls(signedEngine(asked), delivery(downloaded));
    fireEvent.click(saveButton());
    await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    expect(asked).toEqual([false]);
  });

  it('offers the three answers, the safe one first', async () => {
    renderControls(signedEngine(), delivery(downloaded));
    fireEvent.click(saveButton());
    const dialog = await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
    const names = [...dialog.querySelectorAll('button')].map((b) => b.textContent);
    expect(names).toEqual(['Cancel', 'Save a copy', 'Save anyway']);
  });

  describe('keyboard focus', () => {
    it('keeps Save focusable while it runs, so the focus is not thrown to the page', async () => {
      let release: () => void = () => {};
      const gate = new Promise<void>((resolve) => (release = resolve));
      let calls = 0;
      renderControls(fakeEngine({ saveToBytes: async () => (calls++, await gate, bytes) }), delivery(downloaded));
      const button = saveButton();
      button.focus();
      fireEvent.click(button);

      await screen.findByText('Saving…');
      // A `disabled` button would lose the focus; aria-disabled keeps it.
      expect((button as HTMLButtonElement).disabled).toBe(false);
      expect(button.getAttribute('aria-disabled')).toBe('true');
      expect(globalThis.document.activeElement).toBe(button);

      // Pressing it again while it runs does nothing.
      fireEvent.click(button);
      await act(async () => release());
      await screen.findByText('Downloaded a.pdf.');
      expect(calls).toBe(1);
      expect(button.getAttribute('aria-disabled')).toBe('false');
      expect(globalThis.document.activeElement).toBe(button);
    });

    it.each(['Cancel', 'Save a copy', 'Save anyway'])('returns to Save when the signature question closes with %s', async (answer) => {
      renderControls(signedEngine(), delivery(downloaded));
      fireEvent.click(saveButton());
      await screen.findByRole('dialog', { name: SIGNATURE_TITLE });
      fireEvent.click(screen.getByRole('button', { name: answer }));
      await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
      await waitFor(() => expect(globalThis.document.activeElement).toBe(saveButton()));
    });
  });

  it('shows "Unsaved changes." when an edit follows a successful save', async () => {
    let session: SaveSession | undefined;
    renderControls(fakeEngine({ saveToBytes: async () => bytes }), delivery({ how: 'wrote', name: 'a.pdf' }), (s) => (session = s));
    fireEvent.click(saveButton());
    await screen.findByText('Saved over a.pdf.');

    act(() => session?.markDirty());
    expect(await screen.findByText('Unsaved changes.')).toBeTruthy();
    expect(screen.queryByText('Saved over a.pdf.')).toBeNull();
  });
});
